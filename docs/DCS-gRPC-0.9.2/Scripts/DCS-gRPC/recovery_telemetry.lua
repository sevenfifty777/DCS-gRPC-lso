-- Bounded, callback-atomic recovery telemetry engine.
-- DCS APIs, clocks and scheduling are injected so the state machine can be
-- exercised deterministically outside DCS without production fault hooks.

local M = {}

local STATUS_VALID = 1
local STATUS_NOT_FOUND = 2
local STATUS_ID_MISMATCH = 3
local STATUS_READ_ERROR = 4
local STATUS_INVALID_DATA = 5

local LIFECYCLE_ACTIVE = 1
local LIFECYCLE_UNKNOWN = 2
local LIFECYCLE_EXPIRED = 3
local LIFECYCLE_STOPPED = 4
local LIFECYCLE_EPOCH_MISMATCH = 5

local LOSS_NONE = 1
local LOSS_RETENTION_EXPIRED = 2
local LOSS_CAPACITY_OVERFLOW = 3
local LOSS_MIXED = 4

local MAX_SAFE_INTEGER = 9007199254740991
local TOMBSTONE_SECONDS = 30
local TOMBSTONE_CAPACITY = 128
local MAX_TEXT_BYTES = 128

local function finite(value)
  return type(value) == "number" and value == value and value ~= math.huge and value ~= -math.huge
end

local function validVector(value)
  return type(value) == "table" and finite(value.x) and finite(value.y) and finite(value.z)
end

local function validRawTransform(value)
  if type(value) ~= "table" or type(value.position) ~= "table" then
    return false
  end
  local p = value.position
  return finite(p.lat) and finite(p.lon) and finite(p.alt) and finite(p.u) and finite(p.v)
    and validVector(value.positionNorth) and validVector(value.forward)
    and validVector(value.right) and validVector(value.up) and validVector(value.velocity)
end

local function textIsValid(value)
  return type(value) == "string" and #value > 0 and #value <= MAX_TEXT_BYTES
    and not string.find(value, "[%z\1-\31\127]")
end

local function newRing(capacity)
  return { items = {}, head = 1, count = 0, capacity = capacity }
end

local function ringIndex(ring, offset)
  return ((ring.head + offset - 2) % ring.capacity) + 1
end

local function ringFirst(ring)
  if ring.count == 0 then return nil end
  return ring.items[ring.head]
end

local function ringLast(ring)
  if ring.count == 0 then return nil end
  return ring.items[ringIndex(ring, ring.count)]
end

local function ringRemoveFirst(ring)
  if ring.count == 0 then return nil end
  local value = ring.items[ring.head]
  ring.items[ring.head] = nil
  ring.head = (ring.head % ring.capacity) + 1
  ring.count = ring.count - 1
  return value
end

local function ringAppend(recovery, snapshot)
  local ring = recovery.ring
  if ring.count == ring.capacity then
    local removed = ringRemoveFirst(ring)
    recovery.capacityOverflowCount = recovery.capacityOverflowCount + 1
    recovery.capacityEvictedThrough = removed.sequence
  end
  local index = ringIndex(ring, ring.count + 1)
  ring.items[index] = snapshot
  ring.count = ring.count + 1
  if ring.count > recovery.highWaterMark then recovery.highWaterMark = ring.count end
end

local function cleanRetention(recovery, now, retentionSeconds)
  local removed = 0
  while recovery.ring.count > 0 do
    local first = ringFirst(recovery.ring)
    if now - first.captureTime <= retentionSeconds then break end
    ringRemoveFirst(recovery.ring)
    removed = removed + 1
  end
  if removed > 0 then
    recovery.retentionExpirationCount = recovery.retentionExpirationCount + removed
    recovery.retentionEvictedThrough = recovery.ring.count > 0
      and (ringFirst(recovery.ring).sequence - 1) or recovery.sequence
  end
end

-- A read with after_sequence = N is an implicit acknowledgment of every
-- sequence <= N: the client already has that data and only needs it again on
-- retry of the *current* request, whose snapshots all have sequence > N. So
-- purging up to (and not past) after_sequence never removes data this or a
-- retried call would still need, while keeping ring occupancy close to what
-- LSO has actually consumed instead of the full configured capacity.
local function purgeAcknowledged(recovery, afterSequence)
  local lastRemoved = nil
  while recovery.ring.count > 0 do
    local first = ringFirst(recovery.ring)
    if first.sequence > afterSequence then break end
    lastRemoved = ringRemoveFirst(recovery.ring)
  end
  if lastRemoved then
    recovery.ackEvictedThrough = lastRemoved.sequence
  end
end

local function monotonicUs(engine)
  if not engine.monotonicTimeNs then return nil end
  local ok, value = pcall(engine.monotonicTimeNs)
  if not ok or not finite(value) then return nil end
  return math.floor(value / 1000)
end

local function reportObservationError(engine, expectedName, stage, detail)
  if not engine.reportObservationError then return end
  pcall(engine.reportObservationError, expectedName, stage, tostring(detail))
end

local function observation(engine, expectedName, expectedId, callbackStartedUs)
  local startedUs = monotonicUs(engine)
  local result = {
    status = STATUS_READ_ERROR,
    expectedName = expectedName,
    expectedId = expectedId,
  }
  if startedUs and callbackStartedUs then result.readStartedOffsetUs = startedUs - callbackStartedUs end

  local okResolve, unit = pcall(function()
    return engine.getUnitByName(expectedName)
  end)
  if not okResolve then
    result.status = STATUS_READ_ERROR
    reportObservationError(engine, expectedName, "resolve", unit)
  elseif unit == nil then
    result.status = STATUS_NOT_FOUND
  else
    local okId, resolvedId = pcall(function()
      return engine.getUnitId(unit)
    end)
    local resolvedIdDetail = resolvedId
    if okId then resolvedId = tonumber(resolvedId) end
    if not okId or not finite(resolvedId) then
      result.status = STATUS_READ_ERROR
      reportObservationError(engine, expectedName, "id", resolvedIdDetail)
    else
      resolvedId = math.floor(resolvedId)
      result.resolvedId = resolvedId
      if resolvedId ~= expectedId then
        result.status = STATUS_ID_MISMATCH
      else
        local okTransform, rawTransform = pcall(function()
          return engine.exportRawTransform(unit)
        end)
        if not okTransform then
          result.status = STATUS_READ_ERROR
          reportObservationError(engine, expectedName, "transform", rawTransform)
        elseif not validRawTransform(rawTransform) then
          result.status = STATUS_INVALID_DATA
        else
          result.status = STATUS_VALID
          result.rawTransform = rawTransform
        end
      end
    end
  end

  local finishedUs = monotonicUs(engine)
  if finishedUs and callbackStartedUs then result.readFinishedOffsetUs = finishedUs - callbackStartedUs end
  return result
end

local function countActiveCarriers(engine)
  local seen = {}
  local count = 0
  for _, recovery in pairs(engine.recoveries) do
    if not seen[recovery.carrierKey] then
      seen[recovery.carrierKey] = true
      count = count + 1
    end
  end
  return count
end

local function activeCount(engine)
  local count = 0
  for _ in pairs(engine.recoveries) do count = count + 1 end
  return count
end

local function addTombstone(engine, recovery, status, now)
  local filtered = {}
  for _, handle in ipairs(engine.tombstoneOrder) do
    if handle ~= recovery.handle then table.insert(filtered, handle) end
  end
  engine.tombstoneOrder = filtered
  engine.tombstones[recovery.handle] = {
    owner = recovery.owner,
    sourceEpoch = engine.sourceEpoch,
    lifecycleStatus = status,
    expiresAt = now + TOMBSTONE_SECONDS,
  }
  table.insert(engine.tombstoneOrder, recovery.handle)
  while #engine.tombstoneOrder > TOMBSTONE_CAPACITY do
    local oldest = table.remove(engine.tombstoneOrder, 1)
    engine.tombstones[oldest] = nil
  end
end

local function cleanTombstones(engine, now)
  local kept = {}
  for _, handle in ipairs(engine.tombstoneOrder) do
    local tombstone = engine.tombstones[handle]
    if tombstone and tombstone.expiresAt > now then
      table.insert(kept, handle)
    else
      engine.tombstones[handle] = nil
    end
  end
  engine.tombstoneOrder = kept
end

local function expireRecoveries(engine, now)
  local expired = {}
  for handle, recovery in pairs(engine.recoveries) do
    if recovery.leaseExpiresAt <= now then table.insert(expired, handle) end
  end
  for _, handle in ipairs(expired) do
    local recovery = engine.recoveries[handle]
    engine.recoveries[handle] = nil
    addTombstone(engine, recovery, LIFECYCLE_EXPIRED, now)
  end
end

local function resetCaptureContinuity(engine)
  engine.lastCaptureTime = nil
  engine.observedGap = nil
  engine.lastCaptureDurationUs = nil
end

local function validateConfig(config)
  assert(config.capacity >= 1 and config.capacity <= 600, "invalid capacity")
  assert(config.maxBatchSize >= 1 and config.maxBatchSize <= 100, "invalid max batch size")
  assert(config.maxActiveRecoveries >= 1 and config.maxActiveRecoveries <= 64, "invalid recovery limit")
  assert(config.maxActiveCarriers >= 1 and config.maxActiveCarriers <= 32, "invalid carrier limit")
end

function M.new(options)
  local config = options.config
  validateConfig(config)
  return {
    sourceEpoch = assert(options.sourceEpoch),
    config = config,
    now = assert(options.now),
    getUnitByName = assert(options.getUnitByName),
    getUnitId = assert(options.getUnitId),
    exportRawTransform = assert(options.exportRawTransform),
    monotonicTimeNs = options.monotonicTimeNs,
    reportObservationError = options.reportObservationError,
    ensureScheduled = options.ensureScheduled,
    recoveries = {},
    tombstones = {},
    tombstoneOrder = {},
    captureTick = 0,
    lastCaptureTime = nil,
    observedGap = nil,
    missedCaptureIntervals = 0,
    lastCaptureDurationUs = nil,
  }
end

local function lifecycleResponse(engine, params)
  local tombstone = engine.tombstones[params.recoveryHandle]
  if tombstone then
    if tombstone.owner ~= params.owner then return nil, "PERMISSION_DENIED" end
    if params.expectedSourceEpoch ~= "" and params.expectedSourceEpoch ~= tombstone.sourceEpoch then
      return LIFECYCLE_EPOCH_MISMATCH
    end
    return tombstone.lifecycleStatus
  end
  if params.expectedSourceEpoch ~= "" and params.expectedSourceEpoch ~= engine.sourceEpoch then
    return LIFECYCLE_EPOCH_MISMATCH
  end
  return LIFECYCLE_UNKNOWN
end

function M.start(engine, params)
  local now = engine.now()
  cleanTombstones(engine, now)
  expireRecoveries(engine, now)
  if not textIsValid(params.owner) or not textIsValid(params.recoveryHandle)
    or not textIsValid(params.aircraftName) or not textIsValid(params.carrierName)
    or not finite(params.aircraftId) or params.aircraftId <= 0
    or not finite(params.carrierId) or params.carrierId <= 0 then
    return nil, "INVALID_ARGUMENT", "handle, owner, unit names and positive DCS IDs are required"
  end

  local existing = engine.recoveries[params.recoveryHandle]
  if existing then
    if existing.owner ~= params.owner then
      return nil, "PERMISSION_DENIED", "recovery handle is owned by another client"
    end
    if existing.aircraftName ~= params.aircraftName or existing.aircraftId ~= params.aircraftId
      or existing.carrierName ~= params.carrierName or existing.carrierId ~= params.carrierId then
      return nil, "ALREADY_EXISTS", "recovery handle is already bound to a different unit pair"
    end
    existing.leaseExpiresAt = now + engine.config.leaseSeconds
    if engine.ensureScheduled then
      local ok, scheduled, scheduleError = pcall(engine.ensureScheduled)
      if not ok or scheduled ~= true then
        return nil, "INTERNAL", "failed to schedule recovery telemetry capture: "
          .. tostring(scheduleError or scheduled)
      end
    end
    return {
      sourceEpoch = engine.sourceEpoch,
      recoveryHandle = existing.handle,
      leaseExpiresAt = existing.leaseExpiresAt,
      configuredPeriod = engine.config.periodSeconds,
      capacity = engine.config.capacity,
      retentionSeconds = engine.config.retentionSeconds,
      alreadyActive = true,
    }
  end

  local tombstone = engine.tombstones[params.recoveryHandle]
  if tombstone then
    if tombstone.owner ~= params.owner then
      return nil, "PERMISSION_DENIED", "recovery handle is owned by another client"
    end
    return nil, "ALREADY_EXISTS",
      "recovery handle was recently stopped or expired; use a new handle or wait for tombstone expiry"
  end

  if activeCount(engine) >= engine.config.maxActiveRecoveries then
    return nil, "RESOURCE_EXHAUSTED", "maximum active recovery count reached"
  end
  local carrierKey = tostring(params.carrierId) .. "\0" .. params.carrierName
  local carrierAlreadyActive = false
  for _, recovery in pairs(engine.recoveries) do
    if recovery.carrierKey == carrierKey then
      carrierAlreadyActive = true
      break
    end
  end
  if not carrierAlreadyActive and countActiveCarriers(engine) >= engine.config.maxActiveCarriers then
    return nil, "RESOURCE_EXHAUSTED", "maximum active carrier count reached"
  end

  local recovery = {
    handle = params.recoveryHandle,
    owner = params.owner,
    aircraftName = params.aircraftName,
    aircraftId = math.floor(params.aircraftId),
    carrierName = params.carrierName,
    carrierId = math.floor(params.carrierId),
    carrierKey = carrierKey,
    sequence = 0,
    ring = newRing(engine.config.capacity),
    leaseExpiresAt = now + engine.config.leaseSeconds,
    capacityOverflowCount = 0,
    retentionExpirationCount = 0,
    retentionEvictedThrough = 0,
    capacityEvictedThrough = 0,
    ackEvictedThrough = 0,
    lastDiagnosticsAt = nil,
    snapshotsProduced = 0,
    invalidSnapshotsProduced = 0,
    snapshotsServed = 0,
    readBatches = 0,
    lastBatchSize = 0,
    highWaterMark = 0,
  }
  if activeCount(engine) == 0 then resetCaptureContinuity(engine) end
  engine.recoveries[recovery.handle] = recovery
  if engine.ensureScheduled then
    local ok, scheduled, scheduleError = pcall(engine.ensureScheduled)
    if not ok or scheduled ~= true then
      engine.recoveries[recovery.handle] = nil
      resetCaptureContinuity(engine)
      return nil, "INTERNAL", "failed to schedule recovery telemetry capture: "
        .. tostring(scheduleError or scheduled)
    end
  end
  return {
    sourceEpoch = engine.sourceEpoch,
    recoveryHandle = recovery.handle,
    leaseExpiresAt = recovery.leaseExpiresAt,
    configuredPeriod = engine.config.periodSeconds,
    capacity = engine.config.capacity,
    retentionSeconds = engine.config.retentionSeconds,
    alreadyActive = false,
  }
end

function M.captureTick(engine, now)
  cleanTombstones(engine, now)
  expireRecoveries(engine, now)
  if activeCount(engine) == 0 then
    resetCaptureContinuity(engine)
    return false
  end
  if engine.captureTick >= MAX_SAFE_INTEGER - 1 then
    return nil, "capture tick exhausted exact Lua integer range"
  end

  engine.captureTick = engine.captureTick + 1
  if engine.lastCaptureTime then
    local gap = now - engine.lastCaptureTime
    engine.observedGap = gap
    local missed = math.floor((gap / engine.config.periodSeconds) + 0.000001) - 1
    if missed > 0 then engine.missedCaptureIntervals = engine.missedCaptureIntervals + missed end
  end
  engine.lastCaptureTime = now

  local callbackStartedUs = monotonicUs(engine)
  local carrierCache = {}
  local produced = {}
  for _, recovery in pairs(engine.recoveries) do
    cleanRetention(recovery, now, engine.config.retentionSeconds)
    local carrier = carrierCache[recovery.carrierKey]
    if not carrier then
      carrier = observation(engine, recovery.carrierName, recovery.carrierId, callbackStartedUs)
      carrierCache[recovery.carrierKey] = carrier
    end
    local aircraft = observation(engine, recovery.aircraftName, recovery.aircraftId, callbackStartedUs)
    recovery.sequence = recovery.sequence + 1
    local snapshot = {
      recoveryHandle = recovery.handle,
      sourceEpoch = engine.sourceEpoch,
      sequence = recovery.sequence,
      captureTick = engine.captureTick,
      captureTime = now,
      aircraft = aircraft,
      carrier = carrier,
    }
    recovery.snapshotsProduced = recovery.snapshotsProduced + 1
    if aircraft.status ~= STATUS_VALID or carrier.status ~= STATUS_VALID then
      recovery.invalidSnapshotsProduced = recovery.invalidSnapshotsProduced + 1
    end
    ringAppend(recovery, snapshot)
    table.insert(produced, snapshot)
  end

  local callbackFinishedUs = monotonicUs(engine)
  if callbackStartedUs and callbackFinishedUs then
    local duration = callbackFinishedUs - callbackStartedUs
    engine.lastCaptureDurationUs = duration
    for _, snapshot in ipairs(produced) do snapshot.captureDurationUs = duration end
  end
  return true
end

local function diagnostics(engine, recovery, now)
  return {
    lastCaptureTime = engine.lastCaptureTime,
    configuredPeriod = engine.config.periodSeconds,
    observedGap = engine.observedGap,
    missedCaptureIntervals = engine.missedCaptureIntervals,
    snapshotsProduced = recovery and recovery.snapshotsProduced or 0,
    invalidSnapshotsProduced = recovery and recovery.invalidSnapshotsProduced or 0,
    snapshotsServed = recovery and recovery.snapshotsServed or 0,
    retentionExpirationCount = recovery and recovery.retentionExpirationCount or 0,
    capacityOverflowCount = recovery and recovery.capacityOverflowCount or 0,
    highWaterMark = recovery and recovery.highWaterMark or 0,
    activeRecoveries = activeCount(engine),
    activeCarriers = countActiveCarriers(engine),
    lastCaptureDurationUs = engine.lastCaptureDurationUs,
    readBatches = recovery and recovery.readBatches or 0,
    lastBatchSize = recovery and recovery.lastBatchSize or 0,
    sourceAge = engine.lastCaptureTime and (now - engine.lastCaptureTime) or nil,
  }
end

function M.read(engine, params)
  local now = engine.now()
  cleanTombstones(engine, now)
  expireRecoveries(engine, now)
  if not textIsValid(params.owner) or not textIsValid(params.recoveryHandle)
    or not finite(params.afterSequence) or params.afterSequence < 0
    or params.afterSequence > MAX_SAFE_INTEGER
    or not finite(params.limit) or params.limit < 0 then
    return nil, "INVALID_ARGUMENT", "invalid recovery telemetry read request"
  end
  local recovery = engine.recoveries[params.recoveryHandle]
  if not recovery then
    local lifecycle, errorType = lifecycleResponse(engine, params)
    if errorType then return nil, errorType, "recovery handle is owned by another client" end
    return {
      sourceEpoch = engine.sourceEpoch,
      recoveryHandle = params.recoveryHandle,
      lifecycleStatus = lifecycle,
      oldestAvailableSequence = 0,
      newestAvailableSequence = 0,
      nextAfterSequence = params.afterSequence,
      lossReason = LOSS_NONE,
      overflowCount = 0,
      configuredPeriod = engine.config.periodSeconds,
      capacity = engine.config.capacity,
      retentionSeconds = engine.config.retentionSeconds,
      snapshots = {},
      diagnostics = diagnostics(engine, nil, now),
      leaseExpiresAt = 0,
      readTime = now,
    }
  end
  if recovery.owner ~= params.owner then
    return nil, "PERMISSION_DENIED", "recovery handle is owned by another client"
  end
  if params.expectedSourceEpoch ~= "" and params.expectedSourceEpoch ~= engine.sourceEpoch then
    return {
      sourceEpoch = engine.sourceEpoch,
      recoveryHandle = recovery.handle,
      lifecycleStatus = LIFECYCLE_EPOCH_MISMATCH,
      oldestAvailableSequence = 0,
      newestAvailableSequence = recovery.sequence,
      nextAfterSequence = params.afterSequence,
      lossReason = LOSS_NONE,
      overflowCount = recovery.capacityOverflowCount,
      configuredPeriod = engine.config.periodSeconds,
      capacity = engine.config.capacity,
      retentionSeconds = engine.config.retentionSeconds,
      snapshots = {},
      diagnostics = diagnostics(engine, recovery, now),
      leaseExpiresAt = recovery.leaseExpiresAt,
      readTime = now,
    }
  end

  recovery.leaseExpiresAt = now + engine.config.leaseSeconds
  cleanRetention(recovery, now, engine.config.retentionSeconds)
  purgeAcknowledged(recovery, params.afterSequence)
  local first = ringFirst(recovery.ring)
  local last = ringLast(recovery.ring)
  local oldest = first and first.sequence or 0
  local newest = last and last.sequence or 0
  local lossReason = LOSS_NONE
  local evictedThrough = math.max(
    recovery.retentionEvictedThrough, recovery.capacityEvictedThrough, recovery.ackEvictedThrough)
  if params.afterSequence < evictedThrough then
    local retentionLoss = params.afterSequence < recovery.retentionEvictedThrough
    local capacityLoss = params.afterSequence
      < math.max(recovery.capacityEvictedThrough, recovery.ackEvictedThrough)
    if retentionLoss and capacityLoss then
      lossReason = LOSS_MIXED
    elseif retentionLoss then
      lossReason = LOSS_RETENTION_EXPIRED
    elseif capacityLoss then
      lossReason = LOSS_CAPACITY_OVERFLOW
    end
  end
  local limit = math.floor(params.limit)
  if limit == 0 then limit = engine.config.maxBatchSize end
  limit = math.min(limit, engine.config.maxBatchSize)
  local snapshots = {}
  local firstOffset = 1
  if first and params.afterSequence >= first.sequence then
    firstOffset = math.floor(params.afterSequence - first.sequence) + 2
  end
  for offset = firstOffset, recovery.ring.count do
    local snapshot = recovery.ring.items[ringIndex(recovery.ring, offset)]
    if snapshot.sequence > params.afterSequence then
      table.insert(snapshots, snapshot)
      if #snapshots >= limit then break end
    end
  end
  recovery.snapshotsServed = recovery.snapshotsServed + #snapshots
  recovery.readBatches = recovery.readBatches + 1
  recovery.lastBatchSize = #snapshots
  local nextAfter = params.afterSequence
  if #snapshots > 0 then nextAfter = snapshots[#snapshots].sequence end

  -- The full diagnostics block is a fixed per-batch cost that LSO only
  -- consumes once, at the end of the recovery. Sending it on every batch
  -- (thousands per trap) is pure overhead, so it is refreshed on a timer
  -- instead; the wire field is already optional, so omitting it is a no-op
  -- for readers that only look at snapshots.
  -- The small tolerance absorbs floating-point noise in mission time (for example
  -- 1.15 - 0.15 evaluates to just under 1.0) so a read landing exactly on the
  -- interval boundary still refreshes diagnostics.
  local includeDiagnostics = recovery.lastDiagnosticsAt == nil
    or (now - recovery.lastDiagnosticsAt) + 0.000001
      >= (engine.config.diagnosticsIntervalSeconds or 1.0)
  local diagnosticsPayload = nil
  if includeDiagnostics then
    diagnosticsPayload = diagnostics(engine, recovery, now)
    recovery.lastDiagnosticsAt = now
  end

  return {
    sourceEpoch = engine.sourceEpoch,
    recoveryHandle = recovery.handle,
    lifecycleStatus = LIFECYCLE_ACTIVE,
    oldestAvailableSequence = oldest,
    newestAvailableSequence = newest,
    nextAfterSequence = nextAfter,
    lossReason = lossReason,
    overflowCount = recovery.capacityOverflowCount,
    configuredPeriod = engine.config.periodSeconds,
    capacity = engine.config.capacity,
    retentionSeconds = engine.config.retentionSeconds,
    snapshots = snapshots,
    diagnostics = diagnosticsPayload,
    leaseExpiresAt = recovery.leaseExpiresAt,
    readTime = now,
  }
end

function M.stop(engine, params)
  local now = engine.now()
  cleanTombstones(engine, now)
  expireRecoveries(engine, now)
  if not textIsValid(params.owner) or not textIsValid(params.recoveryHandle) then
    return nil, "INVALID_ARGUMENT", "invalid recovery telemetry stop request"
  end
  local recovery = engine.recoveries[params.recoveryHandle]
  if not recovery then
    local lifecycle, errorType = lifecycleResponse(engine, params)
    if errorType then return nil, errorType, "recovery handle is owned by another client" end
    return {
      sourceEpoch = engine.sourceEpoch,
      recoveryHandle = params.recoveryHandle,
      lifecycleStatus = lifecycle,
    }
  end
  if recovery.owner ~= params.owner then
    return nil, "PERMISSION_DENIED", "recovery handle is owned by another client"
  end
  if params.expectedSourceEpoch ~= "" and params.expectedSourceEpoch ~= engine.sourceEpoch then
    return {
      sourceEpoch = engine.sourceEpoch,
      recoveryHandle = recovery.handle,
      lifecycleStatus = LIFECYCLE_EPOCH_MISMATCH,
    }
  end
  engine.recoveries[recovery.handle] = nil
  addTombstone(engine, recovery, LIFECYCLE_STOPPED, now)
  if activeCount(engine) == 0 then resetCaptureContinuity(engine) end
  return {
    sourceEpoch = engine.sourceEpoch,
    recoveryHandle = recovery.handle,
    lifecycleStatus = LIFECYCLE_STOPPED,
  }
end

return M
