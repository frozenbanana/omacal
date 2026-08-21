use keyring::Entry;

const SERVICE: &str = "omacal";
const LEGACY_SERVICE: &str = "omarcal";

pub fn password_key(account_id: &str) -> String {
    format!("account:{account_id}:password")
}

pub fn set_password(account_id: &str, password: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
    entry.set_password(password).map_err(|e| e.to_string())
}

pub fn get_password(account_id: &str) -> Result<String, String> {
    let entry = Entry::new(SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(pw) => Ok(pw),
        Err(keyring::Error::NoEntry) => {
            // Fallback to legacy omarcal service and migrate
            let legacy =
                Entry::new(LEGACY_SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
            match legacy.get_password() {
                Ok(pw) => {
                    let _ = entry.set_password(&pw);
                    Ok(pw)
                }
                Err(e) => Err(e.to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn delete_password(account_id: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
