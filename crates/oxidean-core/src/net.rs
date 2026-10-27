//! Shared IP classification for egress (SSRF) policy and
//! forwarded-header trust decisions.
//!
//! [`is_public_ip`] rejects private, loopback, link-local, unspecified,
//! multicast, CGNAT, benchmarking, documentation, reserved, and
//! transition-mechanism (v4-mapped, NAT64, 6to4, Teredo) ranges on both
//! families. It backs admin-supplied provider `base_url` bounds, the
//! webhook / git-mirror egress policy in `oxidean-api-core::egress`,
//! and X-Forwarded-For trust in `oxidean-api` (a peer may append an
//! honest XFF hop when it is not publicly routable).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Loopback on either family, including v4-mapped and v4-compatible
/// `::ffff:127.x` / `::7f00:x` forms.
pub fn is_loopback_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4().is_some_and(|v4| v4.is_loopback()),
    }
}

/// Publicly routable classification — the allow-list used by every
/// outbound-egress bound. Returns `false` for anything that is not an
/// internet-routable unicast address.
pub fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(mapped) => is_public_v4(&mapped),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(v4: &Ipv4Addr) -> bool {
    let o = v4.octets();
    !(
        o[0] == 0                                    // 0/8 "this network"
        || v4.is_private()                           // 10/8, 172.16/12, 192.168/16
        || v4.is_loopback()                          // 127/8
        || v4.is_link_local()                        // 169.254/16 (incl. metadata)
        || v4.is_multicast()                         // 224/4
        || v4.is_broadcast()                         // 255.255.255.255
        || o[0] >= 240                               // 240/4 reserved
        || (o[0] == 100 && (o[1] & 0xC0) == 64)      // 100.64/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)   // 192.0.0/24 IETF protocols
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)   // 192.0.2/24 TEST-NET-1
        || (o[0] == 198 && (o[1] & 0xFE) == 18)      // 198.18/15 benchmarking
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)// 198.51.100/24 TEST-NET-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113) // 203.0.113/24 TEST-NET-3
    )
}

fn is_public_v6(v6: &Ipv6Addr) -> bool {
    let seg = v6.segments();
    !(
        v6.is_unspecified()                          // ::
        || v6.is_loopback()                          // ::1
        || v6.is_multicast()                         // ff00::/8
        || v6.is_unique_local()                      // fc00::/7
        || v6.is_unicast_link_local()                // fe80::/10
        || seg[0] <= 0x00ff                          // ::/8 reserved
        || (seg[0] == 0x0064 && seg[1] == 0xff9b)    // 64:ff9b::/96 NAT64
        || seg[0] == 0x2002                          // 2002::/16 6to4
        || (seg[0] == 0x2001 && seg[1] == 0x0000)    // 2001::/32 Teredo
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)    // 2001:db8::/32 doc
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_public_ipv4_ranges() {
        for s in [
            "127.0.0.1",
            "10.0.0.9",
            "172.16.5.5",
            "192.168.0.1",
            "169.254.169.254",
            "100.64.7.7",
            "192.0.0.9",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.9",
            "203.0.113.9",
            "0.0.0.0",
            "224.0.0.1",
            "240.1.2.3",
            "255.255.255.255",
        ] {
            assert!(!is_public_ip(&s.parse().unwrap()), "{s}");
        }
    }

    #[test]
    fn non_public_ipv6_ranges() {
        for s in [
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "ff02::1",
            "64:ff9b::7f00:1",
            "2002:7f00:1::",
            "2001::7f00:1",
            "1::",
        ] {
            assert!(!is_public_ip(&s.parse().unwrap()), "{s}");
        }
    }

    #[test]
    fn public_literals_accepted() {
        for s in ["8.8.8.8", "1.1.1.1", "9.0.0.1", "2606:4700:4700::1111"] {
            assert!(is_public_ip(&s.parse().unwrap()), "{s}");
        }
    }

    #[test]
    fn loopback_normalized_across_families() {
        assert!(is_loopback_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_loopback_ip(&"::1".parse().unwrap()));
        assert!(is_loopback_ip(&"::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback_ip(&"10.0.0.1".parse().unwrap()));
        assert!(!is_loopback_ip(&"8.8.8.8".parse().unwrap()));
    }
}
