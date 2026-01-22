use syster::parser::sysml::{SysMLParser, Rule};
use pest::Parser;
use syster::syntax::parser::parse_with_result;
use syster::syntax::SyntaxFile;
use syster::syntax::sysml::ast::enums::Element;
use std::path::Path;

fn main() {
    let text = r#"package ModelingMetadata {
    enum def StatusKind {
        enum open;
        enum closed;
    }
    
    metadata def StatusInfo {
        attribute status : StatusKind;
    }
}

package Test {
    import ModelingMetadata::*;
    
    part myPart {
        @StatusInfo {
            status = StatusKind::closed;
        }
    }
}"#;

    println!("=== PARSED AST ===");
    let result = parse_with_result(text, Path::new("test.sysml"));
    let syntax = result.content.unwrap();
    
    if let SyntaxFile::SysML(sysml) = &syntax {
        for element in &sysml.elements {
            print_element(element, 0);
        }
    }
}

fn print_element(element: &Element, indent: usize) {
    let prefix = "  ".repeat(indent);
    match element {
        Element::Package(pkg) => {
            println!("{}Package: {:?}", prefix, pkg.name);
            for elem in &pkg.elements {
                print_element(elem, indent + 1);
            }
        }
        Element::Definition(def) => {
            println!("{}Definition: {:?} ({:?})", prefix, def.name, def.kind);
            println!("{}  relationships: {:?}", prefix, def.relationships);
            for member in &def.body {
                match member {
                    syster::syntax::sysml::ast::enums::DefinitionMember::Usage(u) => {
                        println!("{}  Usage member: {:?} ({:?})", prefix, u.name, u.kind);
                        println!("{}    expression_refs: {:?}", prefix, u.expression_refs);
                        println!("{}    relationships: {:?}", prefix, u.relationships);
                        print_usage_body(&u.body, indent + 2);
                    }
                    _ => {}
                }
            }
        }
        Element::Usage(usage) => {
            println!("{}Usage: {:?} ({:?})", prefix, usage.name, usage.kind);
            println!("{}  expression_refs: {:?}", prefix, usage.expression_refs);
            println!("{}  relationships: {:?}", prefix, usage.relationships);
            print_usage_body(&usage.body, indent + 1);
        }
        _ => {}
    }
}

fn print_usage_body(body: &[syster::syntax::sysml::ast::enums::UsageMember], indent: usize) {
    let prefix = "  ".repeat(indent);
    for member in body {
        match member {
            syster::syntax::sysml::ast::enums::UsageMember::Usage(u) => {
                println!("{}Nested usage: {:?} ({:?})", prefix, u.name, u.kind);
                println!("{}  expression_refs: {:?}", prefix, u.expression_refs);
                println!("{}  relationships: {:?}", prefix, u.relationships);
                print_usage_body(&u.body, indent + 1);
            }
            _ => {}
        }
    }
}
