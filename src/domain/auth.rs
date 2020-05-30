use rpassword;

pub struct Auth {
    pub user: String,
    pub password: Option<String>,
}

impl Auth {
    pub fn ask_password_if_none(mut self) -> Self {
        if self.password.is_none() {
            // Prompt for a password on TTY
            self.password = rpassword::read_password_from_tty(Some("Password: ")).ok();
        }
        self
    }
}
