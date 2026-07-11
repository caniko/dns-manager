{lib}: let
  inherit (builtins) isAttrs isBool isFloat isInt isList isNull isString toJSON toString typeOf;

  indent = depth: lib.concatStrings (lib.replicate depth " ");
  pklString = value: toJSON value;
  pklKey = key: "[${pklString key}]";

  valueToPkl = depth: value:
    if isNull value
    then "null"
    else if isBool value
    then
      if value
      then "true"
      else "false"
    else if isInt value
    then toString value
    else if isFloat value
    then let
      rendered = toString value;
    in
      if builtins.match ".*[.eE].*" rendered != null
      then rendered
      else "${rendered}.0"
    else if isString value || typeOf value == "path"
    then pklString (toString value)
    else if isList value
    then listToPkl depth value
    else if isAttrs value
    then mappingToPkl depth value
    else throw "dns-manager.to-pkl: unsupported Nix value type ${typeOf value}";

  listToPkl = depth: values:
    if values == []
    then "new Listing {}"
    else ''
      new Listing {
      ${lib.concatMapStringsSep "\n" (value: "${indent (depth + 2)}${valueToPkl (depth + 2) value}") values}
      ${indent depth}}'';

  mappingToPkl = depth: attrs:
    if attrs == {}
    then "new Mapping {}"
    else ''
      new Mapping {
      ${lib.concatStringsSep "\n" (lib.mapAttrsToList (key: value: "${indent (depth + 2)}${pklKey key} = ${valueToPkl (depth + 2) value}") attrs)}
      ${indent depth}}'';

  moduleToPkl = attrs:
    if !isAttrs attrs
    then throw "dns-manager.to-pkl: top-level value must be an attrset"
    else
      lib.concatStringsSep "\n" (
        lib.mapAttrsToList (key: value: "${key} = ${valueToPkl 0 value}") attrs
      )
      + "\n";
in {
  inherit moduleToPkl valueToPkl;
}
