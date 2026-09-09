// src/ip_asn.rs
//
// Opt-in peer IP enrichment (country + AS description) backed by iptoasn.com's free,
// license-free "ip2asn-combined" database — chosen instead of MaxMind GeoLite2, which requires
// a license/account. Format: tab-separated `range_start range_end AS_number country_code
// AS_description`, one row per allocated range (IPv4 and IPv6 combined in one file).
//
// The whole database (~717K rows) is parsed once into two sorted-by-range-start in-memory
// vectors (IPv4/IPv6 kept separate to avoid needing a shared 128-bit address space with
// collision handling) and looked up via binary search — cheap enough to run per-peer on every
// torrent-detail request rather than needing its own cache layer on top.
use serde::Serialize;
use std::io::Read;
use std::net::IpAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AsnInfo {
    pub country_code: String,
    pub asn: u32,
    pub as_name: String,
}

#[derive(Debug, Clone)]
struct V4Range {
    start: u32,
    end: u32,
    asn: u32,
    country: String,
    name: String,
}

#[derive(Debug, Clone)]
struct V6Range {
    start: u128,
    end: u128,
    asn: u32,
    country: String,
    name: String,
}

#[derive(Debug, Default)]
pub struct AsnDb {
    v4: Vec<V4Range>,
    v6: Vec<V6Range>,
}

impl AsnDb {
    pub fn len(&self) -> usize {
        self.v4.len() + self.v6.len()
    }

    pub fn is_empty(&self) -> bool {
        self.v4.is_empty() && self.v6.is_empty()
    }

    pub fn lookup(&self, ip: IpAddr) -> Option<AsnInfo> {
        match ip {
            IpAddr::V4(v4) => {
                let target = u32::from(v4);
                let idx = self.v4.partition_point(|r| r.start <= target);
                let r = idx.checked_sub(1).map(|i| &self.v4[i])?;
                (target <= r.end).then(|| AsnInfo {
                    country_code: r.country.clone(),
                    asn: r.asn,
                    as_name: r.name.clone(),
                })
            }
            IpAddr::V6(v6) => {
                let target = u128::from(v6);
                let idx = self.v6.partition_point(|r| r.start <= target);
                let r = idx.checked_sub(1).map(|i| &self.v6[i])?;
                (target <= r.end).then(|| AsnInfo {
                    country_code: r.country.clone(),
                    asn: r.asn,
                    as_name: r.name.clone(),
                })
            }
        }
    }
}

/// Parses a gzip-compressed `ip2asn-combined.tsv.gz` payload. Rows with `asn == 0` are
/// iptoasn.com's "Not routed" / unallocated entries — skipped, since there's nothing useful to
/// report for them (never real torrent peers in practice).
pub fn parse_tsv_gz(bytes: &[u8]) -> anyhow::Result<AsnDb> {
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut text = String::new();
    decoder.read_to_string(&mut text)?;

    let mut v4 = Vec::new();
    let mut v6 = Vec::new();

    for line in text.lines() {
        let mut parts = line.split('\t');
        let (Some(start_s), Some(end_s), Some(asn_s), Some(country), Some(name)) =
            (parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };

        let Ok(asn) = asn_s.parse::<u32>() else { continue };
        if asn == 0 {
            continue;
        }
        let (Ok(start_ip), Ok(end_ip)) = (start_s.parse::<IpAddr>(), end_s.parse::<IpAddr>()) else {
            continue;
        };

        match (start_ip, end_ip) {
            (IpAddr::V4(s), IpAddr::V4(e)) => v4.push(V4Range {
                start: u32::from(s),
                end: u32::from(e),
                asn,
                country: country.to_string(),
                name: name.to_string(),
            }),
            (IpAddr::V6(s), IpAddr::V6(e)) => v6.push(V6Range {
                start: u128::from(s),
                end: u128::from(e),
                asn,
                country: country.to_string(),
                name: name.to_string(),
            }),
            _ => {}
        }
    }

    v4.sort_by_key(|r| r.start);
    v6.sort_by_key(|r| r.start);

    Ok(AsnDb { v4, v6 })
}

/// Shared, hot-swappable database read by peer-list enrichment and written by
/// `engines::ip_asn_updater`. `None` while disabled or before the first successful load.
pub type IpAsnCache = Arc<RwLock<Option<AsnDb>>>;

pub fn new_cache() -> IpAsnCache {
    Arc::new(RwLock::new(None))
}

pub fn lookup(cache: &IpAsnCache, ip_str: &str) -> Option<AsnInfo> {
    let ip: IpAddr = ip_str.parse().ok()?;
    cache.read().as_ref()?.lookup(ip)
}
