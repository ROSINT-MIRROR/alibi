use crate::error::Error;
use crate::witness::archive_today::ArchiveToday;
use crate::witness::credential::Credential;
use crate::witness::ghostarchive::GhostArchive;
use crate::witness::trait_def::Witness;
use crate::witness::wayback::Wayback;

pub fn select(spec: &str, key: &Credential) -> Result<Vec<Box<dyn Witness>>, Error> {
    let mut out: Vec<Box<dyn Witness>> = Vec::new();
    for name in spec.split(',').map(|s| s.trim().to_ascii_lowercase()) {
        match name.as_str() {
            "" => continue,
            "wayback" | "wb" | "webarchive" => out.push(Box::new(Wayback::new(key.clone())?)),
            "archivetoday" | "archive.today" | "at" => out.push(Box::new(ArchiveToday)),
            "ghostarchive" | "ghost" | "ga" => out.push(Box::new(GhostArchive)),
            other => {
                return Err(Error::Config(format!(
                    "unknown witness: {other} (valid: wayback, archivetoday, ghostarchive)"
                )));
            }
        }
    }
    if out.is_empty() {
        return Err(Error::Config("empty --witness list".to_string()));
    }
    Ok(out)
}
