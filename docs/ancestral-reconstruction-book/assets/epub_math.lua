function Para(element)
  if #element.content ~= 1 then
    return nil
  end

  local inline = element.content[1]
  if inline.t ~= "Math" or inline.mathtype ~= "DisplayMath" then
    return nil
  end

  return pandoc.Div({element}, pandoc.Attr("", {"display-math"}))
end

function Link(element)
  if element.target:match("project%-status%.md$") then
    return element.content
  end

  return nil
end
