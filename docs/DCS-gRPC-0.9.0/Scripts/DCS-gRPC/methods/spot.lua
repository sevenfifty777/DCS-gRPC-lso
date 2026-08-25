local net = net

-- DCS doesn't expose a way to lookup spots by ID natively (they are just objects),
-- so we maintain a local table to map an integer ID to the Spot object.
local nextSpotId = 1
local spots = {}

GRPC.methods.createLaser = function(params)
  local unit = Unit.getByName(params.sourceUnitName)
  if unit == nil then
    return GRPC.errorNotFound("source unit not found")
  end

  local offset = {
    x = params.offset.x,
    y = params.offset.y,
    z = params.offset.z
  }
  
  local dir = {
    x = params.direction.x,
    y = params.direction.y,
    z = params.direction.z
  }

  local spot = Spot.createLaser(unit, offset, dir, params.code)
  if spot == nil then
    return GRPC.errorInternal("failed to create laser spot")
  end

  local id = nextSpotId
  nextSpotId = nextSpotId + 1
  spots[id] = spot

  return GRPC.success({ spotId = id })
end

GRPC.methods.createInfraRed = function(params)
  local unit = Unit.getByName(params.sourceUnitName)
  if unit == nil then
    return GRPC.errorNotFound("source unit not found")
  end

  local offset = {
    x = params.offset.x,
    y = params.offset.y,
    z = params.offset.z
  }
  
  local dir = {
    x = params.direction.x,
    y = params.direction.y,
    z = params.direction.z
  }

  local spot = Spot.createInfraRed(unit, offset, dir)
  if spot == nil then
    return GRPC.errorInternal("failed to create infrared spot")
  end

  local id = nextSpotId
  nextSpotId = nextSpotId + 1
  spots[id] = spot

  return GRPC.success({ spotId = id })
end

GRPC.methods.destroySpot = function(params)
  local spot = spots[params.spotId]
  if spot == nil then
    return GRPC.errorNotFound("spot not found")
  end

  spot:destroy()
  spots[params.spotId] = nil

  return GRPC.success({})
end

GRPC.methods.getSpotPoint = function(params)
  local spot = spots[params.spotId]
  if spot == nil then
    return GRPC.errorNotFound("spot not found")
  end

  local p = spot:getPoint()
  if p == nil then
    return GRPC.errorInternal("could not get spot point")
  end

  local lat, lon, alt = coord.LOtoLL(p)

  return GRPC.success({
    position = {
      lat = lat,
      lon = lon,
      alt = alt
    }
  })
end

GRPC.methods.setSpotCode = function(params)
  local spot = spots[params.spotId]
  if spot == nil then
    return GRPC.errorNotFound("spot not found")
  end

  spot:setCode(params.code)

  return GRPC.success({})
end

GRPC.methods.setSpotPoint = function(params)
  local spot = spots[params.spotId]
  if spot == nil then
    return GRPC.errorNotFound("spot not found")
  end

  local vec3 = coord.LLtoLO(params.position.lat, params.position.lon, params.position.alt)
  spot:setPoint(vec3)

  return GRPC.success({})
end

GRPC.methods.getSpotCode = function(params)
  local spot = spots[params.spotId]
  if spot == nil then
    return GRPC.errorNotFound("spot not found")
  end

  return GRPC.success({
    code = spot:getCode()
  })
end

GRPC.methods.getSpotCategory = function(params)
  local spot = spots[params.spotId]
  if spot == nil then
    return GRPC.errorNotFound("spot not found")
  end

  return GRPC.success({
    category = spot:getCategory()
  })
end
