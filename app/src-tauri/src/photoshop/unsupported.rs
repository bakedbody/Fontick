#[derive(Debug, Clone)]
pub struct PlatformClient;

impl PlatformClient {
    pub fn active(_expected_path: Option<&str>) -> Result<Self, String> {
        Err("Photoshop integration is only supported on Windows and macOS".to_string())
    }

    pub fn path(&self) -> Result<String, String> {
        Err("Photoshop integration is only supported on Windows and macOS".to_string())
    }

    pub fn version(&self) -> Result<String, String> {
        Err("Photoshop integration is only supported on Windows and macOS".to_string())
    }

    pub fn do_javascript(&self, _jsx: &str) -> Result<String, String> {
        Err("Photoshop integration is only supported on Windows and macOS".to_string())
    }
}
