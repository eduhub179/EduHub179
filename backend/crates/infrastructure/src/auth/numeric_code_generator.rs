//! Cryptographically secure 6-digit numeric code generator.
//!
//! Dependencies: `rand` crate, `domain` crate (ports).
//! Guarantees:
//! - Uses a thread-local CSPRNG (`rand::thread_rng()`).
//! - Returns a zero-padded 6-digit string (e.g., "042817").
//! - Range: 000000..=999999 (1 million possible codes).

use domain::ports::auth::CodeGenerator;
use rand::Rng;

/// Generates 6-digit numeric one-time codes.
pub struct NumericCodeGenerator;

impl CodeGenerator for NumericCodeGenerator {
    fn generate(&self) -> String {
        let mut rng = rand::thread_rng();
        format!("{:06}", rng.gen_range(0..1_000_000))
    }
}
