--
-- RPC trigger actions
-- https://wiki.hoggitworld.com/view/DCS_singleton_trigger
--

-- All MarkPanels must have a unique ID but there is no way of
-- delegating the creationg of this ID to the game, so we have
-- to have the following code to make sure we always get a new
-- unique id
local MarkId = 0

local function getMarkId()
    local panels =  world.getMarkPanels()
    local idx = MarkId
    if panels then
        local l_max = math.max
        for _,panel in ipairs(panels) do
            idx = l_max(panel.idx, idx)
        end
    end
    idx = idx + 1
    MarkId = idx
    return idx
end

GRPC.methods.outText = function(params)
  trigger.action.outText(params.text, params.displayTime, params.clearView)

  return GRPC.success({})
end

GRPC.methods.outTextForCoalition = function(params)
  if params.coalition == 0 then
    return GRPC.errorInvalidArgument("a specific coalition must be chosen")
  end

  -- Decrement for non zero-indexed gRPC enum
  trigger.action.outTextForCoalition(params.coalition - 1, params.text, params.displayTime, params.clearView)

  return GRPC.success({})
end

GRPC.methods.outTextForGroup = function(params)
  trigger.action.outTextForGroup(params.groupId, params.text, params.displayTime, params.clearView)

  return GRPC.success({})
end

GRPC.methods.outTextForUnit = function(params)
  trigger.action.outTextForUnit(params.unitId, params.text, params.displayTime, params.clearView)

  return GRPC.success({})
end

GRPC.methods.getUserFlag = function(params)
  return GRPC.success({
    value = trigger.misc.getUserFlag(params.flag),
  })
end

GRPC.methods.setUserFlag = function(params)
  trigger.action.setUserFlag(params.flag, params.value)
  return GRPC.success({})
end

GRPC.methods.markToAll = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)
  local idx = getMarkId()

  trigger.action.markToAll(idx, params.text, point, params.readOnly, params.message)

  return GRPC.success({
    id = idx
  })
end

GRPC.methods.markToCoalition = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)
  local idx = getMarkId()

  local coalition = params.coalition - 1 -- Decrement for non zero-indexed gRPC enum
  trigger.action.markToCoalition(idx, params.text, point, coalition, params.readOnly, params.message)

  return GRPC.success({
    id = idx
  })
end

GRPC.methods.markToGroup = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)
  local idx = getMarkId()

  trigger.action.markToGroup(idx, params.text, point, params.groupId, params.readOnly, params.message)

  return GRPC.success({
    id = idx
  })
end

GRPC.methods.removeMark = function(params)
  trigger.action.removeMark(params.id)

  return GRPC.success({})
end

GRPC.methods.markupToAll = function(params)
  local idx = getMarkId()
  local coalition = params.coalition or -1

   -- Number of points is variable so we need to make a table that we unpack
   -- later and add all parameters after the points into it as well
  local packedParams = {}
  for _, value in ipairs(params.points) do
    table.insert(packedParams, coord.LLtoLO(value.lat, value.lon, value.alt))
  end

  table.insert(packedParams, {
    params.borderColor.red,
    params.borderColor.green,
    params.borderColor.blue,
    params.borderColor.alpha
  })
  table.insert(packedParams, {
    params.fillColor.red,
    params.fillColor.green,
    params.fillColor.blue,
    params.fillColor.alpha
  })
  table.insert(packedParams, params.lineType)
  table.insert(packedParams, params.readOnly)
  table.insert(packedParams, params.message)

  trigger.action.markupToAll(params.shape, coalition, idx, unpack(packedParams))

  return GRPC.success({
    id = idx
  })
end

GRPC.methods.markupToCoalition = function(params)
  if params.coalition == 0 then
    return GRPC.errorInvalidArgument("a specific coalition must be chosen")
  end

  params.coalition = params.coalition - 1 -- Decrement for non zero-indexed gRPC enum

  return GRPC.methods.markupToAll(params)

end


GRPC.methods.explosion = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)

  trigger.action.explosion(point, params.power)

  return GRPC.success({})
end

-- gRPC enums should avoid 0 so we increment it there and then subtract by 1
-- here since this enum is zero indexed.
GRPC.methods.smoke = function(params)
  if params.color == 0 then
    return GRPC.errorInvalidArgument("color cannot be unspecified (0)")
  end
  local point = coord.LLtoLO(params.position.lat, params.position.lon, 0)
  local groundPoint = {
    x = point.x,
    y = land.getHeight({x = point.x, y = point.z}),
    z = point.z
  }

  trigger.action.smoke(groundPoint, params.color - 1)

  return GRPC.success({})
end

GRPC.methods.illuminationBomb = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, 0)
  local groundOffsetPoint = {
    x = point.x,
    y = land.getHeight({x = point.x, y = point.z}) + params.position.alt,
    z = point.z
  }

  trigger.action.illuminationBomb(groundOffsetPoint, params.power)

  return GRPC.success({})
end

-- gRPC enums should avoid 0 so we increment it there and then subtract by 1
-- here since this enum is zero indexed.
GRPC.methods.signalFlare = function(params)
  if params.color == 0 then
    return GRPC.errorInvalidArgument("color cannot be unspecified (0)")
  end
  local point = coord.LLtoLO(params.position.lat, params.position.lon, 0)
  local groundPoint = {
    x = point.x,
    y = land.getHeight({x = point.x, y = point.z}),
    z= point.z}

  trigger.action.signalFlare(groundPoint, params.color - 1, params.azimuth)

  return GRPC.success({})
end

GRPC.methods.effectSmokeBig = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon)
  point.y = land.getHeight({x = point.x, y = point.z})
  
  -- The preset maps directly to the enum values in DCS (1 to 8)
  local density = params.density or 1.0
  if density <= 0.0 then density = 1.0 end
  
  local name = params.name
  if not name or name == "" then
    name = tostring(math.random(1000000, 9999999))
  end
  
  trigger.action.effectSmokeBig(point, params.preset, density, name)
  return GRPC.success({})
end

GRPC.methods.effectSmokeStop = function(params)
  trigger.action.effectSmokeStop(params.name)
  return GRPC.success({})
end

GRPC.methods.setUnitInternalCargo = function(params)
  trigger.action.setUnitInternalCargo(params.unitName, params.mass)
  return GRPC.success({})
end

GRPC.methods.activateGroup = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.activateGroup(group)
  return GRPC.success({})
end

GRPC.methods.deactivateGroup = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.deactivateGroup(group)
  return GRPC.success({})
end

GRPC.methods.setGroupAIOn = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.setGroupAION(group)
  return GRPC.success({})
end

GRPC.methods.setGroupAIOff = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.setGroupAIOFF(group)
  return GRPC.success({})
end

GRPC.methods.groupStopMoving = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.groupStopMoving(group)
  return GRPC.success({})
end

GRPC.methods.groupContinueMoving = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.groupContinueMoving(group)
  return GRPC.success({})
end

GRPC.methods.setAITask = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.setAITask(group, params.taskIndex)
  return GRPC.success({})
end

GRPC.methods.pushAITask = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("could not find group")
  end
  trigger.action.pushAITask(group, params.taskIndex)
  return GRPC.success({})
end

GRPC.methods.setMarkupRadius = function(params)
  trigger.action.setMarkupRadius(params.id, params.radius)
  return GRPC.success({})
end

GRPC.methods.setMarkupText = function(params)
  trigger.action.setMarkupText(params.id, params.text)
  return GRPC.success({})
end

GRPC.methods.setMarkupFontSize = function(params)
  trigger.action.setMarkupFontSize(params.id, params.fontSize)
  return GRPC.success({})
end

GRPC.methods.setMarkupColor = function(params)
  trigger.action.setMarkupColor(params.id, {params.color.red, params.color.green, params.color.blue, params.color.alpha})
  return GRPC.success({})
end

GRPC.methods.setMarkupColorFill = function(params)
  trigger.action.setMarkupColorFill(params.id, {params.fillColor.red, params.fillColor.green, params.fillColor.blue, params.fillColor.alpha})
  return GRPC.success({})
end

GRPC.methods.setMarkupTypeLine = function(params)
  trigger.action.setMarkupTypeLine(params.id, params.lineType)
  return GRPC.success({})
end

GRPC.methods.setMarkupPositionEnd = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)
  trigger.action.setMarkupPositionEnd(params.id, point)
  return GRPC.success({})
end

GRPC.methods.setMarkupPositionStart = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)
  trigger.action.setMarkupPositionStart(params.id, point)
  return GRPC.success({})
end

GRPC.methods.lineToAll = function(params)
  local startPoint = coord.LLtoLO(params.startPoint.lat, params.startPoint.lon, params.startPoint.alt)
  local endPoint = coord.LLtoLO(params.endPoint.lat, params.endPoint.lon, params.endPoint.alt)
  local idx = getMarkId()
  local color = {params.color.red, params.color.green, params.color.blue, params.color.alpha}
  local coalition = params.coalition - 1
  trigger.action.lineToAll(coalition, idx, startPoint, endPoint, color, params.lineType, params.readOnly, params.message)
  return GRPC.success({id = idx})
end

GRPC.methods.circleToAll = function(params)
  local center = coord.LLtoLO(params.center.lat, params.center.lon, params.center.alt)
  local idx = getMarkId()
  local color = {params.borderColor.red, params.borderColor.green, params.borderColor.blue, params.borderColor.alpha}
  local fillColor = {params.fillColor.red, params.fillColor.green, params.fillColor.blue, params.fillColor.alpha}
  local coalition = params.coalition - 1
  trigger.action.circleToAll(coalition, idx, center, params.radius, color, fillColor, params.lineType, params.readOnly, params.message)
  return GRPC.success({id = idx})
end

GRPC.methods.rectToAll = function(params)
  local startPoint = coord.LLtoLO(params.startPoint.lat, params.startPoint.lon, params.startPoint.alt)
  local endPoint = coord.LLtoLO(params.endPoint.lat, params.endPoint.lon, params.endPoint.alt)
  local idx = getMarkId()
  local color = {params.borderColor.red, params.borderColor.green, params.borderColor.blue, params.borderColor.alpha}
  local fillColor = {params.fillColor.red, params.fillColor.green, params.fillColor.blue, params.fillColor.alpha}
  local coalition = params.coalition - 1
  trigger.action.rectToAll(coalition, idx, startPoint, endPoint, color, fillColor, params.lineType, params.readOnly, params.message)
  return GRPC.success({id = idx})
end

GRPC.methods.quadToAll = function(params)
  local p1 = coord.LLtoLO(params.p1.lat, params.p1.lon, params.p1.alt)
  local p2 = coord.LLtoLO(params.p2.lat, params.p2.lon, params.p2.alt)
  local p3 = coord.LLtoLO(params.p3.lat, params.p3.lon, params.p3.alt)
  local p4 = coord.LLtoLO(params.p4.lat, params.p4.lon, params.p4.alt)
  local idx = getMarkId()
  local color = {params.borderColor.red, params.borderColor.green, params.borderColor.blue, params.borderColor.alpha}
  local fillColor = {params.fillColor.red, params.fillColor.green, params.fillColor.blue, params.fillColor.alpha}
  local coalition = params.coalition - 1
  trigger.action.quadToAll(coalition, idx, p1, p2, p3, p4, color, fillColor, params.lineType, params.readOnly, params.message)
  return GRPC.success({id = idx})
end

GRPC.methods.textToAll = function(params)
  local startPoint = coord.LLtoLO(params.startPoint.lat, params.startPoint.lon, params.startPoint.alt)
  local idx = getMarkId()
  local color = {params.borderColor.red, params.borderColor.green, params.borderColor.blue, params.borderColor.alpha}
  local fillColor = {params.fillColor.red, params.fillColor.green, params.fillColor.blue, params.fillColor.alpha}
  local coalition = params.coalition - 1
  trigger.action.textToAll(coalition, idx, startPoint, color, fillColor, params.fontSize, params.readOnly, params.text)
  return GRPC.success({id = idx})
end

GRPC.methods.arrowToAll = function(params)
  local startPoint = coord.LLtoLO(params.startPoint.lat, params.startPoint.lon, params.startPoint.alt)
  local endPoint = coord.LLtoLO(params.endPoint.lat, params.endPoint.lon, params.endPoint.alt)
  local idx = getMarkId()
  local color = {params.borderColor.red, params.borderColor.green, params.borderColor.blue, params.borderColor.alpha}
  local fillColor = {params.fillColor.red, params.fillColor.green, params.fillColor.blue, params.fillColor.alpha}
  local coalition = params.coalition - 1
  trigger.action.arrowToAll(coalition, idx, startPoint, endPoint, color, fillColor, params.lineType, params.readOnly, params.message)
  return GRPC.success({id = idx})
end

GRPC.methods.getZone = function(params)
  local zone = trigger.misc.getZone(params.name)
  if zone == nil then
    return GRPC.errorNotFound("zone '" .. params.name .. "' not found")
  end
  return GRPC.success({
    position = GRPC.exporters.position(zone.point),
    radius = zone.radius,
  })
end