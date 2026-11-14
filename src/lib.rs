// src/lib.rs
pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod downloader;
pub mod engines;
pub mod events;
pub mod ip_asn;
pub mod notify;
pub mod stats;
pub mod trakt_client;
pub mod plex_client;
pub mod fetcher;
pub mod web;

pub use api::shared_http_client;
