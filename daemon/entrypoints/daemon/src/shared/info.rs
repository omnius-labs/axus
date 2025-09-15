use valuable::Valuable;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Valuable)]
pub struct AppInfo {
    pub app_name: String,
    pub git_tag: String,
}

impl AppInfo {
    pub fn new(app_name: &str) -> Result<Self> {
        let git_tag = option_env!("GIT_TAG").unwrap_or("unknown").to_string();

        Ok(Self {
            app_name: app_name.to_string(),
            git_tag,
        })
    }
}

impl std::fmt::Display for AppInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "app_name: {:?},", self.app_name)?;
        Ok(())
    }
}
