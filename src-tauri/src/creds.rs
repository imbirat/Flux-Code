use keyring::Entry;

const SERVICE: &str = "Flux Code";

fn account(provider_id: &str) -> String {
    format!("provider/{provider_id}")
}

/// Store a provider API key in Windows Credential Manager.
pub fn set_key(provider_id: &str, key: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &account(provider_id)).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_key(provider_id: &str) -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE, &account(provider_id)).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn delete_key(provider_id: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &account(provider_id)).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
