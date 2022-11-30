use std::str::from_utf8;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

use log::warn;
use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde::Deserialize;
use serde::Serialize;

use wasmedge_http_req::request;
use wasmedge_wasi_socket::nslookup;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Warn);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(HeimdallRoot::default()) });
}}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct HeimdallRootConf {
    redis_nodes: Option<Vec<String>>,
}
impl Default for HeimdallRootConf {
    fn default() -> Self {
        Self { redis_nodes: None }
    }
}

struct HeimdallRoot {
    conf: HeimdallRootConf,
}
impl Default for HeimdallRoot {
    fn default() -> Self {
        Self {
            conf: Default::default(),
        }
    }
}

impl Context for HeimdallRoot {}

impl RootContext for HeimdallRoot {
    fn on_configure(&mut self, _: usize) -> bool {
        // DNS query
        let addrs = nslookup("httpbin.org", "http");
        // Get first result and check if is IPv4 address
        let addr;
        match addrs {
            Ok(vec_addr) => addr = vec_addr[0],
            Err(err) => {
                println!("nslookup: {}", err.to_string());
                addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(34, 203, 186, 29)), 80);
            }
        }

        let mut writer = Vec::new(); //container for body of a response
        let res = request::get(format!("http://{}", addr), &mut writer).unwrap();
        println!("GET");
        println!("Status: {} {}", res.status_code(), res.reason());
        println!("Headers {}", res.headers());
        println!("{}", String::from_utf8_lossy(&writer));

        if let Some(config_bytes) = self.get_plugin_configuration() {
            warn!("#on_configure {}", from_utf8(&config_bytes).unwrap());
            let conf: HeimdallRootConf = serde_json::from_slice(&config_bytes).unwrap();
            // let conf: HeimdallRootConf =
            //     serde_json::from_str(&*String::from_utf8(config_bytes).unwrap()).unwrap();
            self.conf = conf;
            warn!("#on_configure {:?}", self.conf);
        }

        return true;
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn create_http_context(&self, context_id: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(Heimdall {
            context_id: context_id,
            req_buf: None,
            resp_buf: None,
        }))
    }
}

struct Heimdall {
    context_id: u32,
    req_buf: Option<Vec<u8>>,
    resp_buf: Option<Vec<u8>>,
}

impl Context for Heimdall {}

impl HttpContext for Heimdall {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        for (name, value) in &self.get_http_request_headers() {
            warn!("#request_header {} -> {}: {}", self.context_id, name, value);
        }
        // 执行on_http_request_body
        self.set_property(vec!["wasm_process_req_body"], Some(b"true"));
        Action::Continue
    }

    fn on_http_request_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        let body_buf = self.req_buf.get_or_insert(Vec::new());
        fill_buffer(BufferType::HttpRequestBody, body_size, body_buf);

        if end_of_stream {
            if let Ok(body_string) = from_utf8(&body_buf) {
                warn!("#request_body {} -> {}", self.context_id, body_string);
            }
        }
        Action::Continue
    }

    fn on_http_response_headers(&mut self, _: usize, _: bool) -> Action {
        for (name, value) in &self.get_http_response_headers() {
            warn!(
                "#response_header {} <- {}: {}",
                self.context_id, name, value
            );
        }
        // 执行on_http_response_body
        self.set_property(vec!["wasm_process_resp_body"], Some(b"true"));
        Action::Continue
    }

    fn on_http_response_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        let body_buf = self.resp_buf.get_or_insert(Vec::new());
        fill_buffer(BufferType::HttpResponseBody, body_size, body_buf);

        if end_of_stream {
            if let Ok(body_string) = from_utf8(&body_buf) {
                warn!("#response_body {} -> {}", self.context_id, body_string);
            }
        }

        Action::Continue
    }
}

fn fill_buffer(src_buffer_type: BufferType, size: usize, dest_buffer: &mut Vec<u8>) {
    if size <= 0 {
        return;
    }

    if let Ok(option_src_buffer) = hostcalls::get_buffer(src_buffer_type, 0, size) {
        if let Some(src_buffer) = option_src_buffer {
            for v in src_buffer {
                dest_buffer.push(v);
            }
        }
    }
}
