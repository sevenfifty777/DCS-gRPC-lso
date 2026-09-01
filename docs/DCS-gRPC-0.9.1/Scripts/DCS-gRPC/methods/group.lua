--
-- RPC unit actions
-- https://wiki.hoggitworld.com/view/DCS_Class_Group
--

local GRPC = GRPC

GRPC.methods.getUnits = function(params)
  -- https://wiki.hoggitworld.com/view/DCS_func_getByName
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group does not exist")
  end

  -- https://wiki.hoggitworld.com/view/DCS_func_getUnits
  local units = group:getUnits()

  local result = {}
  for i, unit in ipairs(units) do
    if params.active == nil or params.active == unit:isActive() then
      result[i] = GRPC.exporters.unit(unit)
    end
  end

  return GRPC.success({units = result})
end

GRPC.methods.groupActivate = function(params)
  -- https://wiki.hoggitworld.com/view/DCS_func_activate
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group does not exist")
  end

  group:activate()

  return GRPC.success({})
end

GRPC.methods.groupDestroy = function(params)
  -- https://wiki.hoggitworld.com/view/DCS_func_destroy
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group does not exist")
  end

  group:destroy()

  return GRPC.success({})
end

GRPC.methods.getGroupSize = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group does not exist")
  end
  return GRPC.success({
    size = group:getSize(),
    initialSize = group:getInitialSize(),
  })
end

GRPC.methods.groupExists = function(params)
  local group = Group.getByName(params.groupName)
  local exists = group ~= nil and group:isExist()
  return GRPC.success({ exists = exists })
end

GRPC.methods.enableEmission = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group not found")
  end
  group:enableEmission(params.enable)
  return GRPC.success({})
end

GRPC.methods.getGroup = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group not found")
  end
  return GRPC.success({
    group = GRPC.exporters.group(group)
  })
end

GRPC.methods.getGroupUnit = function(params)
  local group = Group.getByName(params.groupName)
  if group == nil then
    return GRPC.errorNotFound("group not found")
  end
  local unit = group:getUnit(params.index)
  if unit == nil then
    return GRPC.errorNotFound("unit not found in group at that index")
  end
  return GRPC.success({
    unit = GRPC.exporters.unit(unit)
  })
end