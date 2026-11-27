# Compare DDNS { fqdn, type } writers against each zone's excludeRecords.
# Both result lists contain normalized { fqdn, type, zone, reason } entries.
{lib}: {
  ddnsRecords ? [],
  zones ? {},
}: let
  normalize = name: lib.toLower (lib.removeSuffix "." name);
  addressType = type: type == "A" || type == "AAAA";
  inZone = zone: fqdn: fqdn == zone || lib.hasSuffix ".${zone}" fqdn;
  zoneNames =
    lib.sort (a: b: builtins.stringLength a > builtins.stringLength b)
    (map normalize (builtins.attrNames zones));

  # Match render/sync.rs: a dotted owner outside its zone is not relative.
  excludedFqdn = zone: name: let
    bare = normalize name;
  in
    if name == "" || name == "@"
    then zone
    else if inZone zone bare
    then bare
    else if lib.hasSuffix "." name
    then throw "owner ${name} is outside zone ${zone}"
    else "${bare}.${zone}";

  # Attribute names deduplicate the normalized keys and give a stable order.
  byKey = records:
    builtins.listToAttrs (map (record: {
        name = builtins.toJSON [record.zone record.fqdn record.type];
        value = record;
      })
      records);
  writers = byKey (lib.concatMap (record: let
    fqdn = normalize record.fqdn;
    type = lib.toUpper record.type;
  in
    lib.optional (addressType type) {
      inherit fqdn type;
      zone = lib.findFirst (zone: inZone zone fqdn) null zoneNames;
    })
  ddnsRecords);
  exclusions = byKey (lib.concatLists (lib.mapAttrsToList (zoneName: settings: let
    zone = normalize zoneName;
  in
    lib.concatMap (record: let
      type = lib.toUpper record.type;
    in
      lib.optional (addressType type) {
        inherit zone type;
        fqdn = excludedFqdn zone record.name;
      }) (settings.excludeRecords or []))
  zones));
  difference = a: b: builtins.attrValues (builtins.removeAttrs a (builtins.attrNames b));
in {
  missingExcludes = map (record:
    record
    // {
      reason =
        if record.zone == null
        then "no managed zone owns this FQDN"
        else "missing ${record.type} exclude";
    }) (difference writers exclusions);
  orphanExcludes =
    map (record: record // {reason = "orphan exclude";})
    (difference exclusions writers);
}
