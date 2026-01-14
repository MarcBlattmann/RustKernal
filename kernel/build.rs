//! Build script to automatically detect and embed .pa app files

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("apps_generated.rs");
    
    // Get the apps directory relative to this build script
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let apps_dir = Path::new(&manifest_dir).join("apps");
    
    let mut app_entries = Vec::new();
    let mut app_consts = String::new();
    
    // Scan for .pa files
    if apps_dir.exists() {
        for entry in fs::read_dir(&apps_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            
            if path.extension().map(|e| e == "pa").unwrap_or(false) {
                let file_name = path.file_stem().unwrap().to_str().unwrap();
                let const_name = file_name.to_uppercase().replace("-", "_").replace(" ", "_");
                
                // Read the file content
                let content = fs::read_to_string(&path).unwrap();
                
                // Generate a constant for this app
                app_consts.push_str(&format!(
                    "/// App: {}\npub const {}_PA: &str = r#\"{}\"#;\n\n",
                    file_name, const_name, content
                ));
                
                // Add to registry
                app_entries.push((file_name.to_string(), format!("{}_PA", const_name)));
            }
        }
    }
    
    // Generate the registry function
    let mut registry = String::from(
        "/// Get all available app IDs\npub fn get_app_ids() -> &'static [&'static str] {\n    &["
    );
    for (name, _) in &app_entries {
        registry.push_str(&format!("\"{}\", ", name));
    }
    registry.push_str("]\n}\n\n");
    
    // Generate load function
    let mut load_fn = String::from(
        "/// Load an app by ID\npub fn load_app_by_id(id: &str) -> Option<&'static str> {\n    match id {\n"
    );
    for (name, const_name) in &app_entries {
        load_fn.push_str(&format!("        \"{}\" => Some({}),\n", name, const_name));
    }
    load_fn.push_str("        _ => None,\n    }\n}\n");
    
    // Write the generated file
    let generated = format!(
        "// Auto-generated app definitions from kernel/apps/*.pa\n// DO NOT EDIT - regenerated on build\n\n{}{}{}", 
        app_consts, registry, load_fn
    );
    
    fs::write(&dest_path, generated).unwrap();
    
    // Tell Cargo to rerun if apps folder changes
    println!("cargo:rerun-if-changed=apps/");
}
