GRPC.ActiveWeapons = {}

GRPC.methods.weaponGetLauncher = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  local launcher = w:getLauncher()
  if not launcher then return GRPC.success({}) end
  return GRPC.success({ launcherName = launcher:getName() })
end

GRPC.methods.weaponGetTarget = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  local target = w:getTarget()
  if not target then return GRPC.success({}) end
  return GRPC.success({ targetName = target:getName() })
end

GRPC.methods.weaponGetCategory = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  -- gRPC enum for object category starts at 1
  return GRPC.success({ category = w:getCategory() + 1 })
end

GRPC.methods.weaponGetDesc = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  -- DCS getDesc returns a table. We encode it to JSON string.
  return GRPC.success({ descJson = net.lua2json(w:getDesc()) })
end

GRPC.methods.weaponGetPosition = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  local pos = w:getPosition()
  if not pos then return GRPC.errorNotFound("could not get position") end

  local lat, lon, alt = coord.LOtoLL(pos.p)
  return GRPC.success({
    position = {
      position = { lat = lat, lon = lon, alt = alt },
      forward = pos.x,
      up = pos.y,
      right = pos.z
    }
  })
end

GRPC.methods.weaponGetVelocity = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  local v = w:getVelocity()
  if not v then return GRPC.errorNotFound("could not get velocity") end
  return GRPC.success({
    velocity = { x = v.x, y = v.y, z = v.z }
  })
end

GRPC.methods.weaponInAir = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  return GRPC.success({ inAir = w:inAir() })
end

GRPC.methods.weaponIsExist = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.success({ isExist = false }) end
  return GRPC.success({ isExist = w:isExist() })
end

GRPC.methods.weaponDestroy = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  w:destroy()
  return GRPC.success({})
end

GRPC.methods.weaponGetCoalition = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  -- gRPC enum for Coalition starts at 1, DCS starts at 0 (Neutral = 0)
  return GRPC.success({ coalition = w:getCoalition() + 1 })
end

GRPC.methods.weaponGetCountry = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  return GRPC.success({ country = w:getCountry() })
end

GRPC.methods.weaponGetName = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  return GRPC.success({ name = w:getName() })
end

GRPC.methods.weaponGetTypeName = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  return GRPC.success({ typeName = w:getTypeName() })
end

GRPC.methods.weaponGetPoint = function(params)
  local w = GRPC.ActiveWeapons and GRPC.ActiveWeapons[params.name]
  if not w then return GRPC.errorNotFound("weapon not found") end
  local p = w:getPoint()
  if not p then return GRPC.errorNotFound("could not get point") end
  local lat, lon, alt = coord.LOtoLL(p)
  return GRPC.success({
    point = { lat = lat, lon = lon, alt = alt }
  })
end
