use repolayer::parser::idl::protobuf::ProtobufParser;
use std::path::PathBuf;

#[test]
fn extracts_services_and_methods() {
    let p = ProtobufParser::new();
    let result = p
        .parse(&PathBuf::from("tests/fixtures/idl/user.proto"))
        .unwrap();
    assert_eq!(result.package, "promotion.member");
    assert_eq!(result.services.len(), 1);
    assert_eq!(result.services[0].name, "MemberBenefitService");
    let method_names: Vec<_> = result.services[0]
        .methods
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert!(method_names.contains(&"GetBenefit"));
    assert!(method_names.contains(&"Redeem"));
}

#[test]
fn extracts_method_input_output_types() {
    let p = ProtobufParser::new();
    let result = p
        .parse(&PathBuf::from("tests/fixtures/idl/user.proto"))
        .unwrap();
    let get_benefit = result.services[0]
        .methods
        .iter()
        .find(|m| m.name == "GetBenefit")
        .unwrap();
    assert_eq!(get_benefit.input, "GetBenefitRequest");
    assert_eq!(get_benefit.output, "GetBenefitResponse");
}

#[test]
fn handles_proto_with_no_services() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.proto");
    std::fs::write(
        &path,
        r#"syntax = "proto3";
package empty.test;
message Foo { int32 x = 1; }
"#,
    )
    .unwrap();
    let p = ProtobufParser::new();
    let result = p.parse(&path).unwrap();
    assert_eq!(result.package, "empty.test");
    assert_eq!(result.services.len(), 0);
}
