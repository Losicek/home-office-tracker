//! Ruční sonda iCloud kontejneru (macOS): musí běžet jako podepsaná .app
//! s entitlementy a profilem, viz CONTRIBUTING.
#[cfg(target_os = "macos")]
fn main() {
    use objc2_foundation::{NSFileManager, NSString};
    let fm = NSFileManager::defaultManager();
    let id = NSString::from_str("iCloud.com.losicek.homeofficetracker");
    match fm.URLForUbiquityContainerIdentifier(Some(&id)) {
        Some(url) => {
            let path = url.path().map(|p| p.to_string()).unwrap_or_default();
            println!("container={path}");
            let dir = std::path::Path::new(&path).join("Data");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("probe.txt"), b"hello").unwrap();
            println!("write=ok");
        }
        None => println!("container=NONE"),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
