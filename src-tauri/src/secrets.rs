use keyring::Entry;

const SERVICE: &str = "omarcal";

pub fn password_key(account_id: &str) -> String {
    format!("account:{account_id}:password")
}

pub fn set_password(account_id: &str, password: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
    entry.set_password(password).map_err(|e| e.to_string())
}

pub fn get_password(account_id: &str) -> Result<String, String> {
    let entry = Entry::new(SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

pub fn delete_password(account_id: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &password_key(account_id)).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
