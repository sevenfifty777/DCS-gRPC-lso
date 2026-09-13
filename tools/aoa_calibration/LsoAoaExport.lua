-- LsoAoaExport.lua — client-side AoA calibration logger for the DCS-gRPC LSO project.
--
-- Runs inside the pilot's own DCS client (Export.lua sandbox) and writes one CSV row every
-- SAMPLE_PERIOD_S with what the flight model and the cockpit say about angle of attack:
--   * LoGetAngleOfAttack(): the flight model's own true AoA, in degrees;
--   * the cockpit AoA indexer lamps (slow / on-speed / fast) as cockpit arguments;
--   * the cockpit AoA gauge value (units) for the T-45C (VNAO) and the F-14 (Heatblur);
--   * the T-45C HUD AoA readout text, when the HUD is showing it;
--   * position, attitude, speeds and the local wind vector, for alignment and diagnosis.
-- The LSO program on the server computes AoA from the velocity vector, pitch and a wind
-- reference; comparing the two series sample by sample (tools/aoa_calibration/align_aoa.py)
-- gives the offset of our computation and the true-AoA band the indexer calls "on speed".
--
-- Install: add ONE line to  %USERPROFILE%\Saved Games\DCS\Scripts\Export.lua  (see README.md):
--   pcall(function() local lsolfs=require('lfs'); dofile(lsolfs.writedir()..'Scripts/LsoAoaExport.lua') end)
-- and copy this file to  %USERPROFILE%\Saved Games\DCS\Scripts\LsoAoaExport.lua.
-- It chains to whatever export hooks were installed before it (Tacview, SRS, SimShaker, DCS-BIOS).
--
-- Output:  %USERPROFILE%\Saved Games\DCS\Logs\lso_aoa_<yyyymmdd-hhmmss>.csv  (one file per session).
-- Requires the server to allow own-ship export (serverSettings.lua: allow_ownship_export = true),
-- which is the same permission SimShaker and Tacview already rely on.

local lfs = require('lfs')

local SAMPLE_PERIOD_S = 0.05        -- 20 Hz, the same cadence as the server-side buffered telemetry
local FLUSH_EVERY_ROWS = 40          -- flush the file every two seconds so a crash loses little

-- Cockpit argument numbers, from the modules' own cockpit definitions / DCS-BIOS:
--   VNAO T-45C v1.0.3 Cockpit/Scripts/mainpanel_init.lua: AoAGauge arg 840 (0..1 = 0..30 units),
--     AOAIndexerV (slow, "V") arg 320, AOAIndexerO (on speed, donut) arg 321, AOAIndexerUp
--     (fast, "^") arg 322; HUD indicator device 10 shows "AOA  xx.x" (units).
--   Heatblur F-14 (DCS-BIOS F-14.lua): PLT_AOA_SLOW 3760, PLT_AOA_OPT 3761, PLT_AOA_FAST 3762,
--     PLT_AOA_UNITS gauge 2003 (0..1 slider; units ~= value * 30, to be confirmed on the trace).
--   ED F/A-18C (DCS-BIOS FA-18C_hornet.lua): AOA_INDEXER_HIGH 4 (green, slow), AOA_INDEXER_NORMAL 5
--     (yellow, on speed), AOA_INDEXER_LOW 6 (red, fast). No cockpit AoA gauge; the HUD alpha
--     readout element name is not known yet, so `hud` stays nil until a first trace is looked at.
local PROFILES = {
    ["T-45"] = { slow = 320, opt = 321, fast = 322, gauge = 840, gauge_scale = 30.0, hud = 10, hook_draw = 25 },
    ["F-14B"] = { slow = 3760, opt = 3761, fast = 3762, gauge = 2003, gauge_scale = 30.0, hud = nil, hook_draw = 1305 },
    ["FA-18C_hornet"] = { slow = 4, opt = 5, fast = 6, gauge = nil, gauge_scale = 1.0, hud = nil, hook_draw = 25 },
}
-- Every F-14 variant shares the same cockpit.
for _, name in ipairs({ "F-14A", "F-14A-135-GR", "F-14A-135-GR-Early", "F-14A-95-GR", "F-14B(U)", "F-14BU" }) do
    PROFILES[name] = PROFILES["F-14B"]
end

local COLUMNS = {
    "model_time_s", "aircraft", "unit_name", "lat", "lon", "alt_msl_m",
    "heading_deg", "pitch_deg", "bank_deg",
    "aoa_true_deg", "tas_mps", "ias_mps", "vertical_speed_mps",
    "wind_x_mps", "wind_y_mps", "wind_z_mps",
    "indexer_slow", "indexer_opt", "indexer_fast", "gauge_raw", "gauge_units", "hud_aoa_units",
    "hook_draw_arg",
}

local previous = {
    start = LuaExportStart,
    activity = LuaExportActivityNextEvent,
    stop = LuaExportStop,
}

local file = nil
local rows_since_flush = 0
local warned = {}

local function log(message)
    if log_write ~= nil then
        pcall(log_write, "LSO-AOA", log.INFO or 1, message)
    end
end

local function warn_once(key, message)
    if not warned[key] then
        warned[key] = true
        log(message)
    end
end

local function csv(value)
    if value == nil then
        return ""
    end
    local kind = type(value)
    if kind == "number" then
        if value ~= value then -- NaN
            return ""
        end
        return string.format("%.4f", value)
    end
    if kind == "boolean" then
        return value and "1" or "0"
    end
    local text = tostring(value)
    if text:find('[,"\n]') then
        text = '"' .. text:gsub('"', '""') .. '"'
    end
    return text
end

local function argument(device, number)
    if device == nil or number == nil then
        return nil
    end
    local ok, value = pcall(function() return device:get_argument_value(number) end)
    if ok and type(value) == "number" then
        return value
    end
    return nil
end

-- Parse the T-45C HUD "AOA  17.3" line out of list_indication(10). The HUD page names the
-- element "AOA" and prints the value with one decimal (Displays/HUD/Indicator/indication_page.lua).
local function hud_aoa_units(profile)
    if profile.hud == nil or list_indication == nil then
        return nil
    end
    local ok, text = pcall(list_indication, profile.hud)
    if not ok or type(text) ~= "string" then
        return nil
    end
    -- list_indication output is a sequence of "-----------------------------------------\n
    -- <name>\n<value>\n" blocks; find the block whose name is exactly AOA.
    for name, value in text:gmatch("\n([%w_]+)\n([^\n]*)") do
        if name == "AOA" then
            local number = tonumber((value:gsub("[^%d%.%-]", "")))
            return number
        end
    end
    return nil
end

local function open_file()
    local stamp = os.date("%Y%m%d-%H%M%S")
    local path = lfs.writedir() .. "Logs\\lso_aoa_" .. stamp .. ".csv"
    local handle, err = io.open(path, "w")
    if handle == nil then
        log("cannot open " .. tostring(path) .. ": " .. tostring(err))
        return nil
    end
    handle:write(table.concat(COLUMNS, ",") .. "\n")
    log("writing " .. path)
    return handle
end

local function sample()
    if file == nil then
        return
    end
    local self_data = LoGetSelfData()
    if self_data == nil or self_data.Name == nil then
        return -- not in a slot (spectator, F10 map without an aircraft, ...)
    end
    local profile = PROFILES[self_data.Name] or {}
    local device = nil
    if GetDevice ~= nil then
        local ok, main_panel = pcall(GetDevice, 0)
        if ok then
            device = main_panel
        else
            warn_once("device0", "GetDevice(0) unavailable: " .. tostring(main_panel))
        end
    end

    local aoa = LoGetAngleOfAttack()
    local wind = (LoGetWindVelocity ~= nil) and LoGetWindVelocity() or nil
    local lla = self_data.LatLongAlt or {}
    local gauge_raw = argument(device, profile.gauge)
    local hook_draw = nil
    if LoGetAircraftDrawArgumentValue ~= nil and profile.hook_draw ~= nil then
        local ok, value = pcall(LoGetAircraftDrawArgumentValue, profile.hook_draw)
        if ok and type(value) == "number" then
            hook_draw = value
        end
    end

    local row = {
        LoGetModelTime(),
        self_data.Name,
        self_data.UnitName,
        lla.Lat,
        lla.Long,
        lla.Alt,
        self_data.Heading and math.deg(self_data.Heading) or nil,
        self_data.Pitch and math.deg(self_data.Pitch) or nil,
        self_data.Bank and math.deg(self_data.Bank) or nil,
        aoa,
        LoGetTrueAirSpeed(),
        LoGetIndicatedAirSpeed(),
        LoGetVerticalVelocity(),
        wind and wind.x or nil,
        wind and wind.y or nil,
        wind and wind.z or nil,
        argument(device, profile.slow),
        argument(device, profile.opt),
        argument(device, profile.fast),
        gauge_raw,
        gauge_raw and (gauge_raw * (profile.gauge_scale or 1.0)) or nil,
        hud_aoa_units(profile),
        hook_draw,
    }
    local cells = {}
    for index = 1, #COLUMNS do
        cells[index] = csv(row[index])
    end
    file:write(table.concat(cells, ",") .. "\n")
    rows_since_flush = rows_since_flush + 1
    if rows_since_flush >= FLUSH_EVERY_ROWS then
        file:flush()
        rows_since_flush = 0
    end
end

function LuaExportStart()
    if previous.start ~= nil then
        pcall(previous.start)
    end
    local ok, handle = pcall(open_file)
    if ok then
        file = handle
    else
        log("open failed: " .. tostring(handle))
        file = nil
    end
end

function LuaExportActivityNextEvent(t)
    local next_t = t + SAMPLE_PERIOD_S
    if previous.activity ~= nil then
        local ok, chained = pcall(previous.activity, t)
        if ok and type(chained) == "number" and chained < next_t then
            next_t = chained
        end
    end
    local ok, err = pcall(sample)
    if not ok then
        warn_once("sample", "sample failed: " .. tostring(err))
    end
    return next_t
end

function LuaExportStop()
    if file ~= nil then
        pcall(function()
            file:flush()
            file:close()
        end)
        file = nil
    end
    if previous.stop ~= nil then
        pcall(previous.stop)
    end
end
