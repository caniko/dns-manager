{checkDdnsExclusions}: let
  writer = fqdn: type: {inherit fqdn type;};
  exclude = name: type: {inherit name type;};
  issue = zone: fqdn: type: reason: {inherit zone fqdn type reason;};
  clean = {
    missingExcludes = [];
    orphanExcludes = [];
  };
in
  assert checkDdnsExclusions {} == clean;
  assert builtins.all (name:
    checkDdnsExclusions {
      ddnsRecords = [(writer "example.test" "A")];
      zones."example.test".excludeRecords = [(exclude name "A")];
    }
    == clean) ["@" "" "EXAMPLE.test."];
  assert checkDdnsExclusions {
    ddnsRecords = [
      (writer "ExAmPle.Test." "a")
      (writer "example.test" "AAAA")
      (writer "HOST.Example.Test." "A")
      (writer "host.example.test" "aaaa")
      (writer "ignored.example.test" "TXT")
    ];
    zones."Example.TEST.".excludeRecords =
      [
        (exclude "@" "a")
        (exclude "" "AAAA")
        (exclude "EXAMPLE.test." "A")
        (exclude "Host" "A")
        (exclude "HOST.Example.Test." "aaaa")
      ]
      ++ map (type: exclude "ignored" type) ["txt" "MX" "CNAME" "SRV" "CAA"];
  }
  == clean;
  assert checkDdnsExclusions {
    ddnsRecords = [
      (writer "unowned.test" "AAAA")
      (writer "Host.Sub.Example.Test." "A")
      (writer "fooexample.test" "A")
      (writer "HOST.SUB.EXAMPLE.TEST" "a")
      (writer "family.example.test" "A")
    ];
    zones = {
      "example.test".excludeRecords = [
        (exclude "host.sub" "A")
        (exclude "family" "AAAA")
        (exclude "FAMILY.Example.Test." "aaaa")
        (exclude "host.sub.example.test." "A")
      ];
      "sub.example.test" = {};
    };
  }
  == {
    missingExcludes = [
      (issue "example.test" "family.example.test" "A" "missing A exclude")
      (issue "sub.example.test" "host.sub.example.test" "A" "missing A exclude")
      (issue null "fooexample.test" "A" "no managed zone owns this FQDN")
      (issue null "unowned.test" "AAAA" "no managed zone owns this FQDN")
    ];
    orphanExcludes = [
      (issue "example.test" "family.example.test" "AAAA" "orphan exclude")
      (issue "example.test" "host.sub.example.test" "A" "orphan exclude")
    ];
  }; true
