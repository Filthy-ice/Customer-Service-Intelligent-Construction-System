use anyhow::Result;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

fn connect(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, String> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS 解析失败 {host}:{port}: {e}"))?;
    for addr in addrs {
        if let Ok(s) = TcpStream::connect_timeout(&addr, timeout) {
            s.set_read_timeout(Some(timeout))
                .map_err(|e| e.to_string())?;
            s.set_write_timeout(Some(timeout))
                .map_err(|e| e.to_string())?;
            return Ok(s);
        }
    }
    Err(format!("无法连接 {host}:{port}"))
}

/// 读取一行 RESP 简单状态/错误（以 \r\n 结尾），最多 256 字节。
fn read_line(sock: &mut TcpStream) -> Result<String, String> {
    let mut buf = Vec::with_capacity(64);
    let mut byte = [0u8; 1];
    while buf.len() < 256 {
        match sock.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n") {
                    break;
                }
            }
            Err(e) => return Err(format!("读取响应失败: {e}")),
        }
    }
    Ok(String::from_utf8_lossy(&buf).trim_end().to_string())
}

fn resp_command(arg: &str, args: &[&str]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len() + 1).into_bytes();
    let push = |s: &str, out: &mut Vec<u8>| {
        out.extend_from_slice(format!("${}\r\n{s}\r\n", s.len()).as_bytes());
    };
    push(arg, &mut out);
    for a in args {
        push(a, &mut out);
    }
    out
}

/// Redis 探测：可选 AUTH，然后 PING 期待 +PONG。成功返回服务端回应描述。
pub fn redis_probe(
    host: &str,
    port: u16,
    password: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let mut sock = connect(host, port, timeout)?;
    if let Some(pw) = password {
        sock.write_all(&resp_command("AUTH", &[pw]))
            .map_err(|e| format!("AUTH 发送失败: {e}"))?;
        let resp = read_line(&mut sock)?;
        if resp.starts_with('-') {
            // "no password is set" 说明服务可达但未启用鉴权——仍算连通
            if !resp.contains("no password") {
                return Err(format!("AUTH 被拒: {resp}"));
            }
        }
    }
    sock.write_all(b"PING\r\n")
        .map_err(|e| format!("PING 发送失败: {e}"))?;
    let resp = read_line(&mut sock)?;
    if resp == "+PONG" {
        Ok("PONG".to_string())
    } else {
        Err(format!("PING 非预期响应: {resp}"))
    }
}

/// MySQL 探测：读取服务端握手包首字节协议版本（期望 10），并解析 server 版本串。
/// 仅证明对端确为 MySQL 服务，不做登录（表级预检在 M2 随驱动接入）。
pub fn mysql_probe(host: &str, port: u16, timeout: Duration) -> Result<String, String> {
    let mut sock = connect(host, port, timeout)?;
    let mut header = [0u8; 4];
    read_exact(&mut sock, &mut header)?;
    let payload_len =
        (header[0] as usize) | ((header[1] as usize) << 8) | ((header[2] as usize) << 16);
    if payload_len == 0 {
        return Err("握手包长度为 0，非 MySQL 协议".into());
    }
    let mut payload = vec![0u8; payload_len.min(255)];
    read_exact(&mut sock, &mut payload)?;
    let protocol_version = payload[0];
    if protocol_version != 10 {
        return Err(format!("协议版本 {protocol_version}，非预期 10"));
    }
    // server version：从 payload[1..] 到首个 NUL
    let end = payload[1..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(payload.len() - 1);
    let server = String::from_utf8_lossy(&payload[1..1 + end]).to_string();
    Ok(format!("MySQL 协议v{protocol_version} server={server}"))
}

fn read_exact(sock: &mut TcpStream, buf: &mut [u8]) -> Result<(), String> {
    let mut got = 0;
    while got < buf.len() {
        match sock.read(&mut buf[got..]) {
            Ok(0) => return Err("连接被过早关闭".into()),
            Ok(n) => got += n,
            Err(e) => return Err(format!("读取失败: {e}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(k: &str) -> Option<String> {
        std::env::var(k).ok().filter(|v| !v.is_empty())
    }

    #[test]
    fn resp_command_encoding() {
        assert_eq!(
            String::from_utf8(resp_command("AUTH", &["ab"])).unwrap(),
            "*2\r\n$4\r\nAUTH\r\n$2\r\nab\r\n"
        );
    }

    // 以下集成测试仅在 source ~/.icewright/dev.env 后生效，默认跳过保持 CI 绿。
    #[test]
    fn redis_probe_local_if_configured() {
        let (Some(host), pass) = (env("IW_REDIS_HOST"), env("REDISCLI_AUTH")) else {
            eprintln!("跳过：未设置 IW_REDIS_HOST");
            return;
        };
        let port = env("IW_REDIS_PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(6379);
        let out = redis_probe(&host, port, pass.as_deref(), Duration::from_secs(3)).unwrap();
        assert_eq!(out, "PONG");
    }

    #[test]
    fn mysql_probe_local_if_configured() {
        let Some(host) = env("IW_MYSQL_HOST") else {
            eprintln!("跳过：未设置 IW_MYSQL_HOST");
            return;
        };
        let port = env("IW_MYSQL_PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(3306);
        let out = mysql_probe(&host, port, Duration::from_secs(3)).unwrap();
        assert!(out.contains("server=8."), "got {out}");
    }
}
