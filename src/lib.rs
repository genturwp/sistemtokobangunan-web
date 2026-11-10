//! stb-web: the SistemTokoBangunan website (RFC-0001, ADR-0003, ADR-0009).

pub mod admin_cli;
pub mod config;
pub mod db;
pub mod lead;
pub mod phone;
pub mod web;

#[cfg(test)]
mod test_support;
