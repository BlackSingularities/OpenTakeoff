fn main() {
    // Ikona i opis pliku .exe (Eksplorator, pasek zadań, „Aplikacje i funkcje”).
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-env-changed=OPENTAKEOFF_VERSION");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::path::Path::new("icon.ico").exists() {
        let mut res = winresource::WindowsResource::new();
        let ver = std::env::var("OPENTAKEOFF_VERSION").unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
        res.set_icon("icon.ico")
            .set("FileDescription", "OpenTakeoff")
            .set("ProductName", "OpenTakeoff")
            .set("CompanyName", "Adam Rędzikowski")
            .set("LegalCopyright", "Adam Rędzikowski · redzikowski.com")
            .set("ProductVersion", &ver)
            .set("FileVersion", &ver);
        if let Err(e) = res.compile() {
            println!("cargo:warning=ikona .exe pominięta: {e}");
        }
    }
    // Wbudowane checklisty: każdy plik checklists/*.json (alfabetycznie) – nowy plik wystarczy dodać do folderu.
    println!("cargo:rerun-if-changed=checklists");
    let dir = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("checklists");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect())
        .unwrap_or_default();
    files.sort();
    let mut code = String::from("/// Zestawy wbudowane: (nazwa pliku, treść) – generowane przez build.rs.\nconst BUILTIN: &[(&str, &str)] = &[\n");
    for p in &files {
        println!("cargo:rerun-if-changed={}", p.display());
        let name = p.file_name().unwrap().to_string_lossy();
        code.push_str(&format!("    ({name:?}, include_str!({:?})),\n", p.display().to_string()));
    }
    code.push_str("];\n");
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("checklists.rs"), code).expect("checklists.rs");
    // Tłumaczenia: lang/<kod>/LC_MESSAGES/opentakeoff.po (te same pliki czyta crates/core/src/lang.rs).
    println!("cargo:rerun-if-changed=../lang");
    let cfg = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_bundled_translations("../lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/main.slint", cfg).expect("slint");
}
