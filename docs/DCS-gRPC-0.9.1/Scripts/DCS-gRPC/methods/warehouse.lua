local function getWarehouse(params)
  local obj
  if params.airbaseName then
    obj = Airbase.getByName(params.airbaseName)
  elseif params.staticName then
    obj = StaticObject.getByName(params.staticName)
  end

  if obj then
    return obj:getWarehouse()
  end
  return nil
end

GRPC.methods.getInventory = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  local inventory = warehouse:getInventory()
  local inventoryJson = net.lua2json(inventory)

  return GRPC.success({
    inventoryJson = inventoryJson
  })
end

GRPC.methods.getItemCount = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  local count = warehouse:getItemCount(params.itemName)

  return GRPC.success({
    count = count
  })
end

GRPC.methods.addItem = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  warehouse:addItem(params.itemName, params.count)

  return GRPC.success({})
end

GRPC.methods.removeItem = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  warehouse:removeItem(params.itemName, params.count)

  return GRPC.success({})
end

GRPC.methods.setItem = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  warehouse:setItem(params.itemName, params.count)

  return GRPC.success({})
end

GRPC.methods.getLiquidAmount = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  local amount = warehouse:getLiquidAmount(params.liquidType)

  return GRPC.success({
    amount = amount
  })
end

GRPC.methods.addLiquid = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  warehouse:addLiquid(params.liquidType, params.amount)

  return GRPC.success({})
end

GRPC.methods.setLiquidAmount = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  warehouse:setLiquidAmount(params.liquidType, params.amount)

  return GRPC.success({})
end

GRPC.methods.getOwner = function(params)
  local warehouse = getWarehouse(params)
  if warehouse == nil then
    return GRPC.errorNotFound("Could not find airbase or static object warehouse")
  end

  local owner = warehouse:getOwner()
  if owner == nil then
    return GRPC.errorNotFound("Warehouse has no owner")
  end

  -- The owner is an Airbase or StaticObject, we need to return its name.
  -- To be safe, we check its type. DCS objects have getCategory().
  -- But we can just use getName().

  local category = owner:getCategory()
  local ownerName = owner:getName()

  if category == Object.Category.BASE then
    return GRPC.success({
      airbaseName = ownerName
    })
  elseif category == Object.Category.STATIC then
    return GRPC.success({
      staticName = ownerName
    })
  else
    -- Fallback
    return GRPC.success({
      staticName = ownerName
    })
  end
end
