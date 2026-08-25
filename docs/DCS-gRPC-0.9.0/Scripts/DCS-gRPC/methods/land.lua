--
-- RPC land actions
-- https://wiki.hoggitworld.com/view/DCS_singleton_land
--

GRPC.methods.getTerrainHeight = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon)
  return GRPC.success({
    height = land.getHeight({x = point.x, y = point.z}),
  })
end

GRPC.methods.getSurfaceType = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon)
  return GRPC.success({
    surfaceType = land.getSurfaceType({x = point.x, y = point.z}),
  })
end

GRPC.methods.isTerrainVisible = function(params)
  local from = coord.LLtoLO(params.from.lat, params.from.lon, params.from.alt or 0)
  local to = coord.LLtoLO(params.to.lat, params.to.lon, params.to.alt or 0)
  return GRPC.success({
    visible = land.isVisible(from, to),
  })
end

GRPC.methods.getClosestPointOnRoads = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon)
  local x, y = land.getClosestPointOnRoads(params.roadType, point.x, point.z)
  local lat, lon = coord.LOtoLL({x = x, y = 0, z = y})
  return GRPC.success({
    position = { lat = lat, lon = lon, alt = 0 },
  })
end

GRPC.methods.getSurfaceHeightWithSeabed = function(params)
  local point = coord.LLtoLO(params.position.lat, params.position.lon)
  local height = land.getSurfaceHeightWithSeabed({x = point.x, y = point.z})
  
  return GRPC.success({
    height = height
  })
end

GRPC.methods.findPathOnRoads = function(params)
  local startPoint = coord.LLtoLO(params.start.lat, params.start.lon)
  local endPoint = coord.LLtoLO(params["end"].lat, params["end"].lon)
  
  local path = land.findPathOnRoads(params.roadType, startPoint.x, startPoint.z, endPoint.x, endPoint.z)
  local pathJson = net.lua2json(path or {})
  
  return GRPC.success({
    pathJson = pathJson
  })
end

GRPC.methods.getIP = function(params)
  local origin = coord.LLtoLO(params.origin.lat, params.origin.lon)
  origin.y = params.origin.alt -- DCS uses y for altitude
  
  local direction = {
    x = params.direction.lat, -- Assuming direction is passed as a vector through these fields
    y = params.direction.alt,
    z = params.direction.lon
  }
  
  local ip = land.getIP(origin, direction, params.maxDist)
  if ip == nil then
    return GRPC.errorNotFound("No terrain intersection found")
  end
  
  local lat, lon, alt = coord.LOtoLL(ip)
  return GRPC.success({
    intersectionPoint = {
      lat = lat,
      lon = lon,
      alt = alt
    }
  })
end

GRPC.methods.profile = function(params)
  local fromPoint = coord.LLtoLO(params.from.lat, params.from.lon)
  local toPoint = coord.LLtoLO(params.to.lat, params.to.lon)
  
  local profileData = land.profile(fromPoint, toPoint)
  local profileJson = net.lua2json(profileData or {})
  
  return GRPC.success({
    profileJson = profileJson
  })
end
