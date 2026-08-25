local group_option_category = {}
group_option_category[1] = "Air"
group_option_category[2] = "Ground"
group_option_category[3] = "Naval"

GRPC.methods.setAlarmState = function(params)
  if params.alarmState == 0 then
    return GRPC.errorInvalidArgument("alarm_state cannot be unspecified (0)")
  end

  local obj
  if params.name.groupName then
    obj = Group.getByName(params.name.groupName)
  elseif  params.name.unitName then
    obj = Unit.getByName(params.name.unitName)
  else
    return GRPC.errorInvalidArgument("No Group or Unit name provided")
  end

  if obj == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end

  local controller = obj:getController()
  local category_id = obj:getCategory()

  local state_id = AI['Option'][group_option_category[category_id]]['id']['ALARM_STATE']

  controller:setOption(state_id, params.alarmState - 1)

  return GRPC.success({})
end

GRPC.methods.getDetectedTargets = function(params)
  local unit = Unit.getByName(params.unitName)
  if unit == nil then
    return GRPC.errorNotFound("Could not find radar unit with name '" .. params.unitName .. "'")
  end

  local controller = Unit.getController(unit)
  local targets
  if params.detectionType == 0 or params.detectionType == nil then
    targets = controller:getDetectedTargets()
  else
    -- int value from https://wiki.hoggitworld.com/view/DCS_func_getDetectedTargets
    targets = controller:getDetectedTargets(params.detectionType)
  end

  if targets == nil then
    return GRPC.success({
      contacts = targets
    })
  end

  local results = {}

  for i, contact in ipairs(targets) do
    local category = Object.getCategory(contact.object)

    if category == nil then
      return GRPC.errorNotFound("Could not find target with id '" .. contact.object:getID() .. "'")
    end

    local result = {
      distance = contact.distance,
      id = contact.object.id_,
      visible = contact.visible,
      target = {}
    }

    --If target is a unit
    if category == 1 then
      if params.includeObject == true then
        result.target.unit = GRPC.exporters.unit( contact.object )
      else
        result.target.object = GRPC.exporters.unknown( contact.object )
      end
    end
    --If target is a weapon
    if category == 2 then
      if params.includeObject == true then
        result.target.weapon = GRPC.exporters.weapon( contact.object )
      else
        result.target.object = GRPC.exporters.unknown( contact.object )
      end
    end

    results[i] = result
  end

  return GRPC.success({
    contacts = results
  })
end

local function getController(params)
  if params.name.groupName then
    local group = Group.getByName(params.name.groupName)
    if group then return group:getController() end
  elseif params.name.unitName then
    local unit = Unit.getByName(params.name.unitName)
    if unit then return unit:getController() end
  end
  return nil
end

GRPC.methods.hasTask = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  return GRPC.success({ hasTask = controller:hasTask() })
end

GRPC.methods.setOnOff = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  controller:setOnOff(params.status)
  return GRPC.success({})
end

GRPC.methods.setOption = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  local val
  if params.value.boolValue ~= nil then
    val = params.value.boolValue
  elseif params.value.intValue ~= nil then
    val = params.value.intValue
  elseif params.value.doubleValue ~= nil then
    val = params.value.doubleValue
  elseif params.value.stringValue ~= nil then
    val = params.value.stringValue
  else
    return GRPC.errorInvalidArgument("Option value must be provided")
  end

  controller:setOption(params.optionId, val)
  return GRPC.success({})
end

GRPC.methods.isTargetDetected = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  local target = Object.getByName(params.targetName)
  if target == nil then
    return GRPC.errorNotFound("Could not find target with provided name")
  end

  local is_detected = controller:isTargetDetected(target)
  return GRPC.success({ isDetected = is_detected })
end

GRPC.methods.knowTarget = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  local target = Object.getByName(params.targetName)
  if target == nil then
    return GRPC.errorNotFound("Could not find target with provided name")
  end

  controller:knowTarget(target, params.type, params.distance)
  return GRPC.success({})
end

GRPC.methods.setTask = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  if type(params.task) ~= "table" then
    return GRPC.errorInvalidArgument("Task must be a valid task table")
  end

  controller:setTask(params.task)
  return GRPC.success({})
end

GRPC.methods.pushTask = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  if type(params.task) ~= "table" then
    return GRPC.errorInvalidArgument("Task must be a valid task table")
  end

  controller:pushTask(params.task)
  return GRPC.success({})
end

GRPC.methods.popTask = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  controller:popTask()
  return GRPC.success({})
end

GRPC.methods.resetTask = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  controller:resetTask()
  return GRPC.success({})
end

GRPC.methods.setCommand = function(params)
  local controller = getController(params)
  if controller == nil then
    return GRPC.errorNotFound("Could not find group or unit with provided name")
  end
  
  if type(params.command) ~= "table" then
    return GRPC.errorInvalidArgument("Command must be a valid command table")
  end

  controller:setCommand(params.command)
  return GRPC.success({})
end