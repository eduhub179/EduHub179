use ::infrastructure::config::Config;
fn main() {
    let config = Config::load();
    println!("{}", config.database_url);
    println!("EduHub179 backend (scaffold) - run cargo in backend/ to build workspace");
}
