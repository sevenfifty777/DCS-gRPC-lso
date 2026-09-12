--
-- Hook actions
-- Docs: /DCS World/API/DCS_ControlAPI.html
--
-- This file is only loaded in the hook (GameGUI) environment, see grpc.lua. `DCS` and `Export`
-- are deliberately resolved at call time (not captured as upvalues at load time) so a load in
-- an environment that lacks them can never permanently bind `nil`.
--

local GRPC = GRPC
local net = net

local OWNSHIP_HOOK_OBSERVED = 1
local OWNSHIP_HOOK_UNAVAILABLE = 2

local function normalizeMechanizationValue(value)
  if type(value) == "boolean" then
    return value and 1 or 0
  end
  if type(value) == "number" then
    return value
  end
  return nil
end

GRPC.methods.getModelTime = function()
  return GRPC.success({ time = DCS.getModelTime() })
end

GRPC.methods.getOwnshipHookState = function()
  local modelTime = DCS.getModelTime()
  local aircraftType = ""
  local ownshipUnitId = nil

  if Export ~= nil and type(Export.LoGetSelfData) == "function" then
    local selfDataOk, selfData = pcall(Export.LoGetSelfData)
    if selfDataOk and type(selfData) == "table" and type(selfData.Name) == "string" then
      aircraftType = selfData.Name
    end
  end

  if Export ~= nil and type(Export.LoGetPlayerPlaneId) == "function" then
    local playerPlaneIdOk, playerPlaneId = pcall(Export.LoGetPlayerPlaneId)
    if playerPlaneIdOk and type(playerPlaneId) == "number" then
      ownshipUnitId = playerPlaneId
    end
  end

  if Export == nil or type(Export.LoGetMechInfo) ~= "function" then
    return GRPC.success({
      observationStatus = OWNSHIP_HOOK_UNAVAILABLE,
      modelTime = modelTime,
      aircraftType = aircraftType,
      ownshipUnitId = ownshipUnitId,
    })
  end

  local mechInfoOk, mechInfo = pcall(Export.LoGetMechInfo)
  if not mechInfoOk or type(mechInfo) ~= "table" or type(mechInfo.hook) ~= "table" then
    return GRPC.success({
      observationStatus = OWNSHIP_HOOK_UNAVAILABLE,
      modelTime = modelTime,
      aircraftType = aircraftType,
      ownshipUnitId = ownshipUnitId,
    })
  end

  local statusValue = normalizeMechanizationValue(mechInfo.hook.status)
  local value = normalizeMechanizationValue(mechInfo.hook.value)
  if statusValue == nil and value == nil then
    return GRPC.success({
      observationStatus = OWNSHIP_HOOK_UNAVAILABLE,
      modelTime = modelTime,
      aircraftType = aircraftType,
      ownshipUnitId = ownshipUnitId,
    })
  end

  return GRPC.success({
    observationStatus = OWNSHIP_HOOK_OBSERVED,
    modelTime = modelTime,
    aircraftType = aircraftType,
    statusValue = statusValue,
    value = value,
    ownshipUnitId = ownshipUnitId,
  })
end

GRPC.methods.getMissionOptions = function()
  local options = DCS.getMissionOptions()
  return GRPC.success({ optionsJson = net.lua2json(options or {}) })
end

GRPC.methods.getCurrentMission = function()
  local mission = DCS.getCurrentMission()
  return GRPC.success({ missionJson = net.lua2json(mission or {}) })
end

GRPC.methods.getAvailableSlots = function(params)
  local slots = DCS.getAvailableSlots(params.coalition)
  return GRPC.success({ slotsJson = net.lua2json(slots or {}) })
end

GRPC.methods.getAvailableCoalitions = function()
  local coalitions = DCS.getAvailableCoalitions()
  return GRPC.success({ coalitionsJson = net.lua2json(coalitions or {}) })
end

GRPC.methods.getMissionResult = function(params)
  local result = DCS.getMissionResult(params.side)
  return GRPC.success({ result = result })
end

GRPC.methods.getUnitProperty = function(params)
  local prop = DCS.getUnitProperty(params.id, params.property)
  return GRPC.success({ propertyValueJson = net.lua2json(prop or {}) })
end

GRPC.methods.getMissionName = function()
  return GRPC.success({name = DCS.getMissionName()})
end

GRPC.methods.getMissionFilename = function()
  return GRPC.success({name = DCS.getMissionFilename()})
end

GRPC.methods.getMissionDescription = function()
  return GRPC.success({description = DCS.getMissionDescription()})
end

GRPC.methods.reloadCurrentMission = function()
  net.load_mission(DCS.getMissionFilename())
  return GRPC.success({})
end

GRPC.methods.loadNextMission = function()
  return GRPC.success({loaded = net.load_next_mission()})
end

GRPC.methods.loadMission = function(params)
  return GRPC.success({loaded = net.load_mission(params.fileName)})
end

GRPC.methods.getPaused = function()
  return GRPC.success({paused = DCS.getPause()})
end

GRPC.methods.setPaused = function(params)
  DCS.setPause(params.paused)
  return GRPC.success({})
end

GRPC.methods.stopMission = function()
  DCS.stopMission()
  return GRPC.success({})
end

GRPC.methods.exitProcess = function()
  DCS.exitProcess()
  return GRPC.success({})
end

GRPC.methods.hookEval = function(params)
  local fn, err = loadstring(params.lua)
  if not fn then
    return GRPC.error("Failed to load Lua code: "..err)
  end

  local ok, result = pcall(fn)
  if not ok then
    return GRPC.error("Failed to execute Lua code: "..result)
  end

  return GRPC.success(net.lua2json(result))
end

GRPC.methods.isMultiplayer = function()
  return GRPC.success({multiplayer = DCS.isMultiplayer()})
end

GRPC.methods.isServer = function()
  return GRPC.success({server = DCS.isServer()})
end

GRPC.methods.banPlayer = function(params)
  if params.id == 1 then
    return GRPC.errorInvalidArgument("Cannot ban the server user")
  end

  local player_id = net.get_player_info(params.id, "id")

  if not player_id then
    return GRPC.errorNotFound("Could not find player with the ID of " .. params.id)
  end

  return GRPC.success({banned = net.banlist_add(params.id, params.period, params.reason)})
end

GRPC.methods.unbanPlayer = function(params)
  return GRPC.success({unbanned = net.banlist_remove(params.ucid)})
end

GRPC.methods.getBannedPlayers = function()
  local result = {}

  for i, detail in ipairs(net.banlist_get()) do
    result[i] = {
      ucid = detail.ucid,
      ipAddress = detail.ipaddr,
      playerName = detail.name,
      reason = detail.reason,
      bannedFrom = detail.banned_from,
      bannedUntil = detail.banned_until
    }
  end

  return GRPC.success({bans = result})
end

GRPC.methods.getUnitType = function(params)
  -- https://wiki.hoggitworld.com/view/DCS_func_getUnitType
  local unit_type = DCS.getUnitType(params.id)
  -- getUnitType returns an empty string if the unit doesn't exist, ensure we catch eventual nils too
  if unit_type == nil or unit_type == "" then
    return GRPC.errorNotFound("unit `" .. tostring(params.id) .. "` does not exist")
  end

  return GRPC.success({type = unit_type})
end

GRPC.methods.getRealTime = function()
  -- https://wiki.hoggitworld.com/view/DCS_func_getRealTime
  return GRPC.success({time = DCS.getRealTime()})
end

GRPC.methods.getBallisticsCount = function()
  local ballistics = Export.LoGetWorldObjects("ballistic")
  local count = 0
  for _ in pairs(ballistics) do count = count + 1 end
  return GRPC.success({count = count})
end

