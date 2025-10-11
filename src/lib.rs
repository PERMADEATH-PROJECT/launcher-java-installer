use serde_json;
use reqwest;
use zip;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

// Handles downloading the JDK package
struct Downloader {
    pub java_version: String,
    pub download_path: String,
    pub java_url: String,
}

// Handles extracting the downloaded JDK archive
struct Extractor {
    pub download_path: String,
    pub extract_path: String,
}

// Handles installing the JDK to the target directory
struct Installer {
    pub extract_path: String,
    pub install_path: String,
}

// Configures environment variables for the JDK
struct EnvironmentVariableConfigurator {
    pub install_path: String,
}

pub struct JavaSetup {
    downloader: Downloader,
    extractor: Extractor,
    installer: Installer,
    env_configurator: EnvironmentVariableConfigurator,
}

impl Downloader {
    pub async fn download(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Download URL: {}", &self.java_url);
        let body = reqwest::get(&self.java_url).await?.text().await?;
        println!("JSON response: {}", &body);
        let json: serde_json::Value = serde_json::from_str(&body)?;

        // Extracts the JDK download link from the JSON response
        if let Some(link_str) = json.as_array()
            .and_then(|array| array.first())
            .and_then(|item| item.get("binaries"))
            .and_then(|binaries| binaries.as_array())
            .and_then(|binaries_array| binaries_array.first())
            .and_then(|binary| binary.get("package"))
            .and_then(|package| package.get("link"))
            .and_then(|link| link.as_str())
        {
            println!("JDK download link: {}", link_str);
            let response = reqwest::get(link_str).await?;
            let mut file = std::fs::File::create(&self.download_path)?;
            let content = response.bytes().await?;
            std::io::copy(&mut content.as_ref(), &mut file)?;
            println!("JDK downloaded to {}", self.download_path);
        } else {
            println!("Download link not found.");
        }
        Ok(())
    }
}

impl Extractor {
    pub fn extract(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Extracting from {} to {}", &self.download_path, &self.extract_path);
        let file = std::fs::File::open(&self.download_path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        // Iterates through the archive and extracts files
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = std::path::Path::new(&self.extract_path).join(file.sanitized_name());

            if (*file.name()).ends_with('/') {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(&p)?;
                    }
                }
                let mut outfile = std::fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }
        println!("JDK extracted to {}", self.extract_path);
        Ok(())
    }
}

// Recursively copies all files and directories from src to dst
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel_path = entry.path().strip_prefix(src).unwrap();
        let dest_path = dst.join(rel_path);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

impl Installer {
    pub fn install(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Installing from {} to {}", &self.extract_path, &self.install_path);
        if Path::new(&self.install_path).exists() {
            fs::remove_dir_all(&self.install_path)?;
        }

        // Finds the JDK directory containing the 'bin' folder
        let mut jdk_dir: Option<std::path::PathBuf> = None;
        for entry in WalkDir::new(&self.extract_path)
            .min_depth(1)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() && entry.path().join("bin").exists() {
                jdk_dir = Some(entry.path().to_path_buf());
                break;
            }
        }

        if let Some(jdk_path) = jdk_dir {
            copy_dir_all(&jdk_path, Path::new(&self.install_path))?;
            println!("JDK installed to {}", self.install_path);
        } else {
            println!("Extracted JDK folder not found.");
        }
        Ok(())
    }
}

impl EnvironmentVariableConfigurator {
    pub unsafe fn configure(&self) -> Result<(), Box<dyn std::error::Error>> {
        let jdk_bin_path = format!("{}\\bin", self.install_path);
        let current_path = std::env::var("PATH").unwrap_or_default();
        println!("Actual PATH: {}", current_path);

        // Update the current process PATH
        if !current_path.contains(&jdk_bin_path) {
            let new_path = format!("{};{}", current_path, jdk_bin_path);
            unsafe {
            std::env::set_var("PATH", &new_path);
            }
            println!("Updated PATH with JDK bin.");
        } else {
            println!("The PATH already contains the JDK bin.");
        }

        // Generates and runs the PowerShell script to update the user's PATH
        let script_content = format!(
            r#"
$jdkPath = "{jdk_bin_path}"
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$jdkPath*") {{
    $newPath = "$userPath;$jdkPath"
    [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
    Write-Host "Updated user's PATH."
}} else {{
    Write-Host "PATH already contains the JDK."
}}
"#);

        // Get main disk
        let main_disk = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        println!("Main disk: {}", main_disk);

        // Get %temp% dir
        let temp_dir = std::env::var("TEMP").unwrap_or_else(|_| format!("{}\\Temp", main_disk).into());
        let script_path = format!("{}\\add_jdk_to_path.ps1", temp_dir);
        println!("Creating PowerShell script at: {}", &script_path);
        fs::write(&script_path, script_content)?;

        let status = std::process::Command::new("powershell")
            .args(&["-ExecutionPolicy", "Bypass", "-File", &script_path])
            .status()?;

        if status.success() {
            println!("Powershell script executed correctly.");
        } else {
            println!("There was an error executing the PowerShell script.");
        }

        Ok(())
    }
}

impl JavaSetup {
    pub fn new(java_version: &str, download_path: &str, extract_path: &str, install_path: &str) -> Self {
        let java_url = format!(
            "https://api.adoptium.net/v3/assets/feature_releases/{}/ga?architecture=x64&os=windows&image_type=jdk",
            java_version
        );
        JavaSetup {
            downloader: Downloader {
                java_version: java_version.to_string(),
                download_path: download_path.to_string(),
                java_url,
            },
            extractor: Extractor {
                download_path: download_path.to_string(),
                extract_path: extract_path.to_string(),
            },
            installer: Installer {
                extract_path: extract_path.to_string(),
                install_path: install_path.to_string(),
            },
            env_configurator: EnvironmentVariableConfigurator {
                install_path: install_path.to_string(),
            },
        }
    }

    pub async fn setup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Format download_path to remove the file name and keep only the directory
        let download_dir = Path::new(&self.downloader.download_path)
            .parent()
            .unwrap_or(Path::new("."))
            .to_str()
            .unwrap_or(".");

        // If the download directory does not exist, create it
        if !Path::new(download_dir).exists() {
            fs::create_dir_all(download_dir)?;
        }

        println!("Starting download...");
        self.downloader.download().await?;
        println!("Extracting...");
        self.extractor.extract()?;
        println!("Installing...");
        self.installer.install()?;
        println!("Configuring environment variables...");
        unsafe {
            self.env_configurator.configure()?;
        }
        println!("Done! Deleting temporary files...");

        if !Path::new(download_dir).exists() {
            println!("No temporary files to delete.");
            return Ok(());
        }

        fs::remove_dir_all(download_dir)?;
        println!("Temporary files deleted.");

        Ok(())
    }
}