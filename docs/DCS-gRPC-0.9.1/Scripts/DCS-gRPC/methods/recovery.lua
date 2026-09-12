--
-- Recovery-specific atomic observation
--

local DRAW_ARGUMENT_NOT_REQUESTED = 1
local DRAW_ARGUMENT_OBSERVED = 2
local DRAW_ARGUMENT_UNAVAILABLE = 3

GRPC.methods.getRecoverySnapshot = function(params)
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
    end
  end

  return GRPC.success({
    time = observedAt,
    carrierRawTransform = GRPC.exporters.rawTransform(carrier),
    aircraftRawTransform = GRPC.exporters.rawTransform(aircraft),
    aircraftDrawArgument = drawArgument,
    sequence = params.sequence,
  })
end
