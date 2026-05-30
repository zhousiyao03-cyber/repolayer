use repolayer::adapters::objc::parse_objc;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse(src: &str) -> repolayer::core::declaration::ParseResult {
    parse_objc(Path::new("test.m"), src.as_bytes())
}

fn find<'a>(decls: &'a [Declaration], name: &str) -> Option<&'a Declaration> {
    for d in decls {
        if d.name == name {
            return Some(d);
        }
        if let Some(x) = find(&d.children, name) {
            return Some(x);
        }
    }
    None
}

const FIXTURE: &str = r#"#import <Foundation/Foundation.h>

@interface Foo : NSObject
@property (nonatomic, strong) NSString *name;
- (void)doThing:(NSString *)arg;
+ (instancetype)create;
@end

@implementation Foo
- (void)doThing:(NSString *)arg { NSLog(@"%@", arg); }
@end

@protocol Drawable
- (void)draw;
@end
"#;

#[test]
fn parses_interface_with_members() {
    let r = parse(FIXTURE);
    assert_eq!(r.language, "objc");

    let foo = r
        .declarations
        .iter()
        .find(|d| d.name == "Foo" && matches!(d.kind, DeclarationKind::Class))
        .expect("Foo interface/impl");

    let prop = find(&foo.children, "name").expect("name property");
    assert!(matches!(prop.kind, DeclarationKind::Property));

    let method = find(&foo.children, "doThing:").expect("doThing: method");
    assert!(matches!(method.kind, DeclarationKind::Method));
    assert_eq!(foo.bases, vec!["NSObject".to_string()]);
}

#[test]
fn parses_class_method_selector() {
    let r = parse(FIXTURE);
    // `+ (instancetype)create;` → selector "create".
    assert!(find(&r.declarations, "create").is_some(), "create method");
}

#[test]
fn parses_implementation_methods() {
    let r = parse(FIXTURE);
    // @implementation Foo also produces a Class decl carrying the definition.
    let impls: Vec<&Declaration> = r
        .declarations
        .iter()
        .filter(|d| d.name == "Foo" && matches!(d.kind, DeclarationKind::Class))
        .collect();
    // One @interface + one @implementation.
    assert_eq!(impls.len(), 2, "expected interface + implementation");
    // At least one of them carries the doThing: method body.
    assert!(
        impls
            .iter()
            .any(|d| find(&d.children, "doThing:").is_some()),
        "doThing: method present"
    );
}

#[test]
fn parses_protocol_as_interface() {
    let r = parse(FIXTURE);
    let drawable = find(&r.declarations, "Drawable").expect("Drawable protocol");
    assert!(matches!(drawable.kind, DeclarationKind::Interface));
    assert!(find(&drawable.children, "draw").is_some(), "draw method");
}

#[test]
fn parses_multipart_selector() {
    let src = r#"@interface Bar : NSObject
- (NSString *)doThing:(NSString *)arg withCount:(NSInteger)n;
@end
"#;
    let r = parse(src);
    let bar = find(&r.declarations, "Bar").expect("Bar");
    assert!(
        find(&bar.children, "doThing:withCount:").is_some(),
        "multipart selector, got: {:?}",
        bar.children.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

#[test]
fn parses_fixture_file_via_dispatch() {
    use repolayer::adapters::parse_file;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/objc/Foo.m");
    let r = parse_file(Path::new(path)).expect("dispatched");
    assert_eq!(r.language, "objc");
    assert!(find(&r.declarations, "Foo").is_some());
}
