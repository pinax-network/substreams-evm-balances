//! Protobuf types for the packages of this repository, generated from
//! `proto/v1` by the `buf.build/anthropics/buffa` plugin (v0.9.2, see
//! `proto/buf.gen.yaml`); run `buf generate` in `proto/` to regenerate. This
//! file is written by hand and keeps the module paths of the earlier prost
//! code.
//!
//! `evm.balance_state.v1` sits behind the default `balance-state` feature.
//! `native/balances` and `erc20/balances` turn it off: their `db_out` links
//! every message of this crate into the WASM (buffa's `json` feature), so a
//! change to the balance-state messages would otherwise change their module
//! hashes.

macro_rules! include_buffa {
    ($package:literal) => {
        include!(concat!($package, ".mod.rs"));
    };
}

pub mod evm {
    pub mod balances {
        pub mod v1 {
            include_buffa!("evm.balances.v1");
        }
    }
    #[cfg(feature = "balance-state")]
    pub mod balance_state {
        pub mod v1 {
            include_buffa!("evm.balance_state.v1");
        }
    }
}
