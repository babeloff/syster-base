//! Debug example to trace scope-aware resolution.

use std::path::Path;
use syster::base::FileId;
use syster::hir::{extract_symbols, SymbolIndex, SymbolKind};
use syster::syntax::parser::parse_with_result;
use syster::syntax::SyntaxFile;

fn main() {
    let source = r#"package SimpleVehicleModel {
    public import Definitions::*;
    
    package Definitions {
        public import AttributeDefinitions::*;
        
        package AttributeDefinitions {
            public import ScalarValues::*;
        }
    }
    
    package VehicleAnalysis {
        calc def ComputeBSFC {
            return : Real;
        }
    }
}"#;

    // Also need the ScalarValues package with Real
    let stdlib_source = r#"package ScalarValues {
    attribute def Real;
}"#;

    let result = parse_with_result(source, Path::new("/test.sysml"));
    let stdlib_result = parse_with_result(stdlib_source, Path::new("/stdlib.sysml"));
    
    let mut index = SymbolIndex::new();
    
    if let Some(SyntaxFile::SysML(sysml)) = result.content {
        let file_id = FileId::new(0);
        let symbols = extract_symbols(file_id, &sysml);
        
        println!("=== Symbols in test file ===");
        for sym in &symbols {
            println!("  {} (kind={:?}, is_public={})", sym.qualified_name, sym.kind, sym.is_public);
        }
        
        index.add_file(file_id, symbols);
    }
    
    if let Some(SyntaxFile::SysML(sysml)) = stdlib_result.content {
        let file_id = FileId::new(1);
        let symbols = extract_symbols(file_id, &sysml);
        
        println!("\n=== Symbols in stdlib file ===");
        for sym in &symbols {
            println!("  {} (kind={:?}, is_public={})", sym.qualified_name, sym.kind, sym.is_public);
        }
        
        index.add_file(file_id, symbols);
    }
    
    // Test resolution from ComputeBSFC's scope
    let scope = "SimpleVehicleModel::VehicleAnalysis::ComputeBSFC";
    println!("\n=== Building resolver for scope: {} ===", scope);
    
    let resolver = index.resolver_for_scope(scope);
    println!("Resolver imports: {:?}", resolver);
    
    // Try to resolve "Real"
    let result = resolver.resolve_type("Real");
    println!("\n=== Resolving 'Real' ===");
    println!("Result: {:?}", result);
}
