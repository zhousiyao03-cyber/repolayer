use repolayer::adapters::idl::thrift::ThriftParser;
use std::path::PathBuf;

#[test]
fn extracts_services_and_methods() {
    let p = ThriftParser::new();
    let result = p
        .parse(&PathBuf::from("tests/fixtures/idl/user.thrift"))
        .unwrap();
    assert_eq!(result.package, "promotion.member");
    assert_eq!(result.services.len(), 1);
    assert_eq!(result.services[0].name, "MemberBenefitService");
    let names: Vec<_> = result.services[0]
        .methods
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert!(names.contains(&"GetBenefit"));
    assert!(names.contains(&"Redeem"));
}

#[test]
fn extracts_method_input_output_types() {
    let p = ThriftParser::new();
    let result = p
        .parse(&PathBuf::from("tests/fixtures/idl/user.thrift"))
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
fn handles_thrift_with_no_services() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.thrift");
    std::fs::write(
        &path,
        r#"namespace go empty.test
struct Foo { 1: i32 x }
"#,
    )
    .unwrap();
    let p = ThriftParser::new();
    let result = p.parse(&path).unwrap();
    assert_eq!(result.package, "empty.test");
    assert_eq!(result.services.len(), 0);
}
