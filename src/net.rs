//! 本机局域网地址探测。
//!
//! 优先用「设备侧回包里记录的命中网卡」（最可靠）；拿不到时用 UDP connect 让系统选路；
//! 都不行才退回枚举网卡——但只在候选唯一时才用，避免多网卡下猜错（与原 C# 版 F5 修复一致）。

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

pub fn is_usable(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    if o[0] == 0 || o[0] == 127 {
        return false;
    }
    if o[0] == 169 && o[1] == 254 {
        return false;
    }
    !ip.is_unspecified() && !ip.is_loopback() && !ip.is_link_local()
}

pub fn usable_local_addresses() -> Vec<Ipv4Addr> {
    let mut list: Vec<Ipv4Addr> = Vec::new();
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for item in interfaces {
            if let IpAddr::V4(v4) = item.ip() {
                if is_usable(v4) && !list.contains(&v4) {
                    list.push(v4);
                }
            }
        }
    }
    list
}

/// 让系统按路由表选一张网卡：UDP `connect` 不真正发包。
pub fn local_address_for(remote: &str, port: u16) -> Option<Ipv4Addr> {
    let remote_ip: Ipv4Addr = remote.trim().parse().ok()?;
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(SocketAddr::from((remote_ip, port))).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if is_usable(v4) => Some(v4),
        _ => None,
    }
}

/// 把监听地址换成浏览器能直接打开的地址：
/// 通配地址（0.0.0.0 / :: / *）替换为本机回环；IPv6 补方括号，否则 URL 不合法。
pub fn browsable_address(address: &str) -> String {
    match address.trim() {
        "" | "0.0.0.0" | "::" | "[::]" | "*" => "127.0.0.1".to_string(),
        other => {
            let host = other.trim_matches(['[', ']']);
            if host.contains(':') && !host.starts_with('[') {
                format!("[{host}]")
            } else {
                host.to_string()
            }
        }
    }
}
