--
-- Recovery-specific atomic observation
--

local DRAW_ARGUMENT_NOT_REQUESTED = 1
local DRAW_ARGUMENT_OBSERVED = 2
local DRAW_ARGUMENT_UNAVAILABLE = 3

-- Wall-clock milliseconds for measuring the callback's own execution time. Prefers the DLL's
-- monotonic clock; falls back to `os.clock()` (CPU time) when the mission environment still
-- exposes `os`; returns nil when neither is available.
local function clockMs()
  if type(GRPC.monotonicMs) == "function" then
    local ok, value = pcall(GRPC.monotonicMs)
    if ok and type(value) == "number" then
      return value
    end
  end
  if os ~= nil and type(os.clock) == "function" then
    return os.clock() * 1000
  end
  return nil
end

GRPC.methods.getRecoverySnapshot = function(params, meta)
  local startedAt = clockMs()

  if params.carrierName == nil or params.carrierName == "" then
    return GRPC.errorInvalidArgument("carrierName must be provided")
  end
  if params.aircraftName == nil or params.aircraftName == "" then
    return GRPC.errorInvalidArgument("aircraftName must be provided")
  end

  local carrier = Unit.getByName(params.carrierName)
  if carrier == nil then
    return GRPC.errorNotFound("carrier unit does not exist")
  end

  local aircraft = Unit.getByName(params.aircraftName)
  if aircraft == nil then
    return GRPC.errorNotFound("aircraft unit does not exist")
  end

  local observedAt = timer.getTime()
  local drawArgument = {
    status = DRAW_ARGUMENT_NOT_REQUESTED
  }

  if params.aircraftDrawArgument ~= nil then
    local ok, value = pcall(function()
      return aircraft:getDrawArgumentValue(params.aircraftDrawArgument)
    end)
    if ok and type(value) == "number" then
      drawArgument.status = DRAW_ARGUMENT_OBSERVED
      drawArgument.value = value
    else
      drawArgument.status = DRAW_ARGUMENT_UNAVAILABLE
      if ok then
        drawArgument.detail = "getDrawArgumentValue returned " .. type(value)
      else
        drawArgument.detail = tostring(value)
      end
    end
  end

  local carrierRawTransform = GRPC.exporters.rawTransform(carrier)
  local aircraftRawTransform = GRPC.exporters.rawTransform(aircraft)

  -- Diagnostics: queue metadata comes from the DLL (see `handleRequest` in grpc.lua); the
  -- execution time covers everything from callback entry until the observation was collected.
  local queueWaitMs, queueDepth
  if type(meta) == "table" then
    if type(meta.queueWaitMs) == "number" then
      queueWaitMs = meta.queueWaitMs
    end
    if type(meta.queueDepthAtEnqueue) == "number" then
      queueDepth = meta.queueDepthAtEnqueue
    end
  end
  local luaExecMs
  if startedAt ~= nil then
    local finishedAt = clockMs()
    if finishedAt ~= nil then
      luaExecMs = math.max(0, finishedAt - startedAt)
    end
  end

  return GRPC.success({
    time = observedAt,
    carrierRawTransform = carrierRawTransform,
    aircraftRawTransform = aircraftRawTransform,
    aircraftDrawArgument = drawArgument,
    sequence = params.sequence,
    queueWaitMs = queueWaitMs,
    luaExecMs = luaExecMs,
    queueDepth = queueDepth,
    dequeuedModelTime = observedAt,
  })
end

--
-- Source-buffered recovery telemetry
--

local telemetryConfig = GRPC.recoveryTelemetry or {}
local telemetryEnabled = telemetryConfig.enabled == true and GRPC.isMissionEnv
local telemetryEngine = nil
local telemetryScheduled = false
local telemetryConsecutiveFailures = 0
local telemetryObservationErrors = {}
local telemetryObservationErrorOrder = {}
local TELEMETRY_OBSERVATION_ERROR_CAPACITY = 128

local function configValue(name, default)
  local value = telemetryConfig[name]
  if value == nil then return default end
  return value
end

local function telemetryError(kind, message)
  if kind == "INVALID_ARGUMENT" then return GRPC.errorInvalidArgument(message) end
  if kind == "ALREADY_EXISTS" then return GRPC.errorAlreadyExists(message) end
  if kind == "PERMISSION_DENIED" then return GRPC.errorPermissionDenied(message) end
  if kind == "RESOURCE_EXHAUSTED" then return GRPC.errorResourceExhausted(message) end
  return GRPC.error(message)
end

local function ensureTelemetryScheduled()
  if telemetryScheduled then return true end
  local function nextCaptureTime(scheduledTime)
    local ok, now = pcall(timer.getTime)
    return ((ok and now) or scheduledTime) + telemetryEngine.state.config.periodSeconds
  end
  local function captureCallback(_, scheduledTime)
    local ok, active, err = pcall(function()
      return telemetryEngine.captureTick(telemetryEngine.state, timer.getTime())
    end)
    if not ok then
      telemetryConsecutiveFailures = telemetryConsecutiveFailures + 1
      if telemetryConsecutiveFailures == 1
        or telemetryConsecutiveFailures == 10
        or telemetryConsecutiveFailures % 100 == 0 then
        GRPC.logError("Recovery telemetry capture failed; retrying (failure "
          .. telemetryConsecutiveFailures .. "): " .. tostring(active))
      end
      return nextCaptureTime(scheduledTime)
    end
    telemetryConsecutiveFailures = 0
    if active == nil then
      telemetryScheduled = false
      GRPC.logError("Recovery telemetry capture stopped: " .. tostring(err))
      return nil
    end
    if not active then
      telemetryScheduled = false
      return nil
    end
    return nextCaptureTime(scheduledTime)
  end

  local ok, scheduleId = pcall(function()
    return timer.scheduleFunction(captureCallback, nil,
      timer.getTime() + configValue("periodSeconds", 0.05))
  end)
  if not ok or scheduleId == nil then
    telemetryScheduled = false
    local message = ok and "scheduleFunction returned no timer id" or tostring(scheduleId)
    GRPC.logError("Failed to schedule recovery telemetry capture: " .. message)
    return nil, message
  end
  telemetryScheduled = true
  return true
end

if telemetryEnabled then
  local module = dofile(GRPC.luaPath .. [[recovery_telemetry.lua]])
  local state = module.new({
    sourceEpoch = GRPC.newSessionId(),
    config = {
      periodSeconds = configValue("periodSeconds", 0.05),
      retentionSeconds = configValue("retentionSeconds", 30),
      capacity = configValue("capacity", 600),
      leaseSeconds = configValue("leaseSeconds", 60),
      maxActiveRecoveries = configValue("maxActiveRecoveries", 16),
      maxActiveCarriers = configValue("maxActiveCarriers", 8),
      maxBatchSize = configValue("maxBatchSize", 100),
      readsPerSecond = configValue("readsPerSecond", 20),
      diagnosticsIntervalSeconds = configValue("diagnosticsIntervalSeconds", 1.0),
    },
    now = timer.getTime,
    getUnitByName = function(name) return Unit.getByName(name) end,
    getUnitId = function(unit) return tonumber(unit:getID()) end,
    exportRawTransform = GRPC.exporters.rawTransform,
    monotonicTimeNs = GRPC.monotonicTimeNs,
    reportObservationError = function(name, stage, detail)
      local boundedDetail = string.sub(tostring(detail), 1, 256)
      local key = tostring(name) .. "\0" .. tostring(stage) .. "\0" .. boundedDetail
      if telemetryObservationErrors[key] then return end
      telemetryObservationErrors[key] = true
      table.insert(telemetryObservationErrorOrder, key)
      while #telemetryObservationErrorOrder > TELEMETRY_OBSERVATION_ERROR_CAPACITY do
        local oldest = table.remove(telemetryObservationErrorOrder, 1)
        telemetryObservationErrors[oldest] = nil
      end
      GRPC.logError("Recovery telemetry read error for " .. tostring(name)
        .. " at " .. tostring(stage) .. ": " .. boundedDetail)
    end,
    ensureScheduled = ensureTelemetryScheduled,
  })
  telemetryEngine = {
    state = state,
    start = module.start,
    read = module.read,
    stop = module.stop,
    captureTick = module.captureTick,
  }
  GRPC.logInfo("Recovery telemetry enabled with source epoch " .. state.sourceEpoch)
end

local function requireTelemetry()
  if not telemetryEnabled then
    return GRPC.errorUnimplemented("recovery telemetry is disabled by server configuration")
  end
  return nil
end

GRPC.methods.startRecoveryTelemetry = function(params)
  local disabled = requireTelemetry()
  if disabled then return disabled end
  local result, kind, message = telemetryEngine.start(telemetryEngine.state, params)
  if not result then return telemetryError(kind, message) end
  return GRPC.success(result)
end

GRPC.methods.readRecoveryTelemetry = function(params)
  local disabled = requireTelemetry()
  if disabled then return disabled end
  local result, kind, message = telemetryEngine.read(telemetryEngine.state, params)
  if not result then return telemetryError(kind, message) end
  return GRPC.success(result)
end

GRPC.methods.stopRecoveryTelemetry = function(params)
  local disabled = requireTelemetry()
  if disabled then return disabled end
  local result, kind, message = telemetryEngine.stop(telemetryEngine.state, params)
  if not result then return telemetryError(kind, message) end
  return GRPC.success(result)
end
