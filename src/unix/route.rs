//! Linux `ip route` parsing for the per-route `quickack` flag.

/// Absolute path; see `sysctl::SYSCTL` for why.
pub const IP: &str = "/usr/sbin/ip";

/// Status flags `ip route show` prints but `ip route change` doesn't accept.
const OUTPUT_ONLY_FLAGS: &[&str] = &["linkdown", "dead", "offload", "trap", "rt_offload", "rt_trap"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultRoute {
    /// Route tokens suitable for `ip route change`, without `quickack`.
    pub spec: Vec<String>,
    pub quickack: bool,
}

impl DefaultRoute {
    fn field(&self, name: &str) -> Option<&str> {
        let i = self.spec.iter().position(|t| t == name)?;
        self.spec.get(i + 1).map(String::as_str)
    }
    pub fn dev(&self) -> Option<&str> {
        self.field("dev")
    }
    pub fn via(&self) -> Option<&str> {
        self.field("via")
    }
}

/// Parses `ip route show default` output.
pub fn parse_default_routes(output: &str) -> Vec<DefaultRoute> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| l.split_whitespace().next() == Some("default"))
        .map(|line| {
            let mut spec = Vec::new();
            let mut quickack = false;
            let mut tokens = line.split_whitespace();
            while let Some(t) = tokens.next() {
                if t == "quickack" {
                    quickack = tokens.next() == Some("1");
                } else if !OUTPUT_ONLY_FLAGS.contains(&t) {
                    spec.push(t.to_string());
                }
            }
            DefaultRoute { spec, quickack }
        })
        .collect()
}

/// Arguments for `ip` to set `quickack` on `route`.
pub fn change_args(route: &DefaultRoute, enable: bool) -> Vec<String> {
    let mut args = vec!["route".to_string(), "change".to_string()];
    args.extend(route.spec.iter().cloned());
    args.push("quickack".into());
    args.push(if enable { "1" } else { "0" }.into());
    args
}

/// NetworkManager dispatcher hook: DHCP renewals and reconnects recreate the
/// default route without `quickack`, so re-apply it whenever a link comes up.
pub const DISPATCHER_SCRIPT: &str = r#"#!/bin/sh
# Managed by tibia-net-optimizer: re-enable TCP quick ACK on default routes
# whenever NetworkManager brings a connection up. Removed by `revert`.
case "$2" in
  up|dhcp4-change) ;;
  *) exit 0 ;;
esac
/usr/sbin/ip route show default | while read -r route; do
  route=$(printf '%s\n' "$route" | sed -e 's/ quickack [0-9]*//' -e 's/ linkdown//')
  /usr/sbin/ip route change $route quickack 1 2>/dev/null
done
exit 0
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const ETH: &str = "default via 192.168.1.1 dev eth0 proto dhcp src 192.168.1.10 metric 100 \n";

    #[test]
    fn parses_a_plain_default_route() {
        let routes = parse_default_routes(ETH);
        assert_eq!(routes.len(), 1);
        let r = &routes[0];
        assert!(!r.quickack);
        assert_eq!(r.dev(), Some("eth0"));
        assert_eq!(r.via(), Some("192.168.1.1"));
        assert_eq!(
            r.spec,
            "default via 192.168.1.1 dev eth0 proto dhcp src 192.168.1.10 metric 100"
                .split(' ')
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn detects_and_strips_quickack() {
        let r = &parse_default_routes("default via 10.0.0.1 dev wlan0 quickack 1 metric 600")[0];
        assert!(r.quickack);
        assert!(!r.spec.iter().any(|t| t == "quickack"), "{:?}", r.spec);
        assert_eq!(r.spec.last().unwrap(), "600");

        let r = &parse_default_routes("default via 10.0.0.1 dev wlan0 quickack 0")[0];
        assert!(!r.quickack);
    }

    #[test]
    fn strips_output_only_flags() {
        let r = &parse_default_routes("default via 10.0.0.1 dev eth1 linkdown")[0];
        assert_eq!(r.spec, vec!["default", "via", "10.0.0.1", "dev", "eth1"]);
    }

    #[test]
    fn handles_multiple_blank_and_non_default_lines() {
        let out = format!("{ETH}\n10.0.0.0/8 via 10.0.0.1 dev eth1\ndefault via 10.8.0.1 dev tun0\n\n");
        let routes = parse_default_routes(&out);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[1].dev(), Some("tun0"));
        assert!(parse_default_routes("").is_empty());
    }

    #[test]
    fn change_args_append_quickack() {
        let r = &parse_default_routes("default via 10.0.0.1 dev eth0 quickack 1")[0];
        assert_eq!(
            change_args(r, true),
            vec!["route", "change", "default", "via", "10.0.0.1", "dev", "eth0", "quickack", "1"]
        );
        assert_eq!(change_args(r, false).last().unwrap(), "0");
    }

    #[test]
    fn dispatcher_script_reapplies_on_up_events() {
        assert!(DISPATCHER_SCRIPT.starts_with("#!/bin/sh\n"));
        assert!(DISPATCHER_SCRIPT.contains("up|dhcp4-change"));
        assert!(DISPATCHER_SCRIPT.contains("quickack 1"));
    }
}
