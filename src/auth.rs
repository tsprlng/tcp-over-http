use lazy_static::lazy_static;
use rand::Rng;
use rand::distr::Alphanumeric;

lazy_static! {
    pub static ref CORRECT_AUTH_HEADER: String = {
        let mut rng = rand::rng();
        let token = rng.sample_iter(Alphanumeric).take(22).collect::<Vec<_>>();
        format!("Bearer {}", String::from_utf8(token).expect("alphanumeric generator should produce valid utf-8"))
    };
}
