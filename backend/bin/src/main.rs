use ::infrastructure::config::Config;
fn main() {
    let config = Config::load();
    let settings = domain::settings::Settings::try_new(config.org_email_domain.clone())
        .expect("ORG_EMAIL_DOMAIN must look like '@179.ru' (leading '@', non-empty)");
    domain::settings::init(settings);
    println!("{}", config.database_url);
    println!("EduHub179 backend (scaffold) - run cargo in backend/ to build workspace");
}
