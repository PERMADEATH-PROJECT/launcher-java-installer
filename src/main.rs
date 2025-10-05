mod lib;
use lib::JavaSetup;

#[tokio::main]
async fn main() {
    // Example usage
    let java_version = "17";
    let download_path = "E:\\Escritorio\\temp\\java_download.zip";
    let extract_path = "E:\\Escritorio\\temp\\java_extract";
    let install_path = "E:\\Escritorio\\temp\\java_install\\jdk-17";

    let mut setup = JavaSetup::new(java_version, download_path, extract_path, install_path);
    if let Err(e) = setup.setup().await {
        eprintln!("Error durante la configuración de Java: {}", e);
    }
}