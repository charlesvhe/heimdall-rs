# APISIX rust wasm插件开发
APISIX支持wasm插件 具体参考 https://apisix.apache.org/docs/apisix/wasm/

官方示例使用go语言开发打包wasm，由于使用proxy-wasm标准，因此可以使用proxy-wasm支持的语言开发wasm插件，支持语言如下：
- AssemblyScript SDK
- C++ SDK
- Go (TinyGo) SDK
- Rust SDK
- Zig SDK

具体参考 https://github.com/proxy-wasm/spec#sdks

# 开发第一个wasm插件
基础环境搭建（安装wasm runtime、添加wasm32-wasi build target） 具体参考 https://wasmedge.org/book/en/write_wasm/rust.html

## 添加proxy-wasm SDK依赖
```
[lib]
crate-type = ["cdylib"]
path = "./src/heimdall.rs"

[dependencies]
proxy-wasm = "0"
log = "0"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## 编写heimdall.rs代码
完整代码备份在heimdall-0.rs，关键代码如下：
```
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

```
## 构建wasm
cargo build --target wasm32-wasi

## 挂载wasm插件
docker run -d --name apisix \
-v `pwd`/apisix/config.yaml:/usr/local/apisix/conf/config.yaml \
-v `pwd`/target/wasm32-wasi/debug/heimdall.wasm:/usr/local/apisix/apisix/plugins/heimdall.wasm \
-v `pwd`/apisix/apisix.yaml:/usr/local/apisix/conf/apisix.yaml \
-p 9080:9080 apache/apisix

## 测试 
http://localhost:9080/httpbin/get

可以看到几个重要的信息，代表我们的插件正常运行：
```
...
2022-11-30 11:26:42 2022/11/30 03:26:42 [notice] 1#1: init wasm vm: wasmtime
...
2022-11-30 11:26:50 2022/11/30 03:26:50 [warn] 50#50: *584 #request_header 2 -> host: localhost:9080, client: 172.17.0.1, server: _, request: "GET /httpbin/get HTTP/1.1", host: "localhost:9080"
...
2022-11-30 11:26:51 2022/11/30 03:26:51 [warn] 50#50: *584 #response_header 2 <- date: Wed, 30 Nov 2022 03:26:51 GMT while reading response header from upstream, client: 172.17.0.1, server: _, request: "GET /httpbin/get HTTP/1.1", upstream: "http://34.203.186.29:80/get", host: "localhost:9080"

```

### 注意事项
- request header阶段配置 `self.set_property(vec!["wasm_process_req_body"], Some(b"true"));` 才能开启on_http_request_body()调用，response同理
  
这是APISIX的约定 具体参考 https://apisix.apache.org/docs/apisix/wasm/

- on_http_request_body、on_http_response_body会被调用多次，每个chuck调用一次，需要缓存拼接内容

具体参考 https://github.com/proxy-wasm/spec/tree/master/abi-versions/vNEXT

- APISIX尚未完整实现proxy-wasm标准

具体参考 https://github.com/api7/wasm-nginx-module/issues/25

- wasm32-wasi标准不支持socket `I/O error: operation not supported on this platform`

具体参考 https://github.com/redis-rs/redis-rs/issues/508

# 使APISIX wasm插件支持socket
wasm32-wasi标准尚不支持socket，但WasmEdge Runtime是支持的。APISIX默认wasm runtime是wasmtime，不支持socket，接下来我们要加入wasmedge runtime。

具体参考 https://github.com/api7/wasm-nginx-module

我们要编译一个含wasmedge的APISIX-Base替换原有的

## 编译apisix-base
我已经从 https://github.com/api7/apisix-build-tools fork了一份并修改了几行代码使其支持wasmedge

具体参考 https://github.com/charlesvhe/apisix-build-tools

执行命令打包：

- RPM：`make package version=1.0.0 image_base=centos image_tag=7 app=apisix-base type=rpm`
  
包地址：apisix/docker-wasm-centos7/apisix-base-1.0.0-0.el7.x86_64.rpm

- DEB：`make package version=1.0.0 image_base=debian image_tag=bullseye-slim app=apisix-base type=deb`
  
包地址：apisix/docker-wasm/apisix-base_1.0.0-0~debianbullseye-slim_amd64.deb

注意：DEB包只能使用IP，不支持使用域名转IP nslookup(os error 85)，建议使用RPM

## 打包APISIX镜像
进入apisix/docker-wasm-centos7目录，运行 `docker build -t charlesvhe/apisix:wasm .`

## 编写socket代码
### 添加wasmedge_wasi_socket依赖
```
[dependencies]
...
wasmedge_http_req  = "0"
wasmedge_wasi_socket = "0"
```

### on_configure中访问外部地址
完整代码备份在heimdall-1.rs，关键代码如下：
```
use wasmedge_http_req::request;
use wasmedge_wasi_socket::nslookup;
...
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
        ...

        return true;
    }
}
```
同样使用 `cargo build --target wasm32-wasi` 构建

## 使用新镜像
docker run -d --name apisix \
-v `pwd`/apisix/config.yaml:/usr/local/apisix/conf/config.yaml \
-v `pwd`/target/wasm32-wasi/debug/heimdall.wasm:/usr/local/apisix/apisix/plugins/heimdall.wasm \
-v `pwd`/apisix/apisix.yaml:/usr/local/apisix/conf/apisix.yaml \
-p 9080:9080 charlesvhe/apisix:wasm

## 测试 
http://localhost:9080/httpbin/get

可以看到几个重要的信息：
```
...
2022-11-30 12:12:44 2022/11/30 04:12:44 [notice] 1#1: init wasm vm: wasmedge
...
2022-11-30 12:13:14 GET
2022-11-30 12:13:14 Status: 200 OK
2022-11-30 12:13:14 Headers {
2022-11-30 12:13:14   Content-Length: 9593
2022-11-30 12:13:14   Content-Type: text/html; charset=utf-8
2022-11-30 12:13:14   Server: gunicorn/19.9.0
2022-11-30 12:13:14 }
...
2022-11-30 12:13:14 <!DOCTYPE html>
2022-11-30 12:13:14 <html lang="en">
2022-11-30 12:13:14 <head>
2022-11-30 12:13:14     <meta charset="UTF-8">
2022-11-30 12:13:14     <title>httpbin.org</title>

```

# 改造redis rust客户端支持wasm
我已经从 https://github.com/redis-rs/redis-rs fork了一份并修改了几行代码使其支持wasmedge
具体参考 https://github.com/charlesvhe/redis-rs/tree/v0.22.1

主要修改
- redis/Cargo.toml

添加wasmedge_wasi_socket依赖 
```
[dependencies]
...
wasmedge_wasi_socket = "0"
```

- redis/src/connection.rs

替换 `std::net::{self, TcpStream, ToSocketAddrs};` 并处理相关编译错误，只需修改6行代码
```
[dependencies]
...
//use std::net::{self, TcpStream, ToSocketAddrs};
use wasmedge_wasi_socket::{self, TcpStream, ToSocketAddrs};

- match TcpStream::connect_timeout(&addr, timeout) {
+ match TcpStream::connect(&addr) {
...
```

**建议：初次测试可以新建一个rust-test工程，用main函数测试**

redis客户端依赖修改后的本地代码
```
[dependencies]
wasmedge_wasi_socket = "0"
redis = { path = "/Volumes/DATA/VSCodeProjects/redis-rs/redis"}
```

```
use wasmedge_wasi_socket::nslookup;
use redis::Commands;

fn main() {
    // connect to redis
    let redis_ip_addr = nslookup("localhost", "").unwrap()[0].ip();
    let client = redis::Client::open(format!("redis://{}:6379/", redis_ip_addr)).unwrap();
    let mut con = client.get_connection().unwrap();
    // throw away the result, just make sure it does not fail
    let _: () = con.set("key", "foobar").unwrap();
    let res: String = con.get("key").unwrap();
    println!("key: {}", res);
}
```
同样使用 `cargo build --target wasm32-wasi` 编译，使用 `wasmedge target/wasm32-wasi/debug/rust-test.wasm` 执行

## APISIX wasm插件使用改造后的redis客户端

同样redis客户端依赖修改后的本地代码
```
[dependencies]
wasmedge_wasi_socket = "0"
redis = { path = "/Volumes/DATA/VSCodeProjects/redis-rs/redis"}
```
on_configure添加redis相关代码，完整代码备份在heimdall-2.rs，关键代码如下：
```
use wasmedge_wasi_socket::nslookup;
use redis::Commands;
...
impl RootContext for HeimdallRoot {
    fn on_configure(&mut self, _: usize) -> bool {
        // connect to redis
        let redis_ip_addr = nslookup("redis", "").unwrap()[0].ip();
        println!("nslookup redis: {}", redis_ip_addr);

        let client = redis::Client::open(format!("redis://{}:6379/", redis_ip_addr)).unwrap();
        let mut con = client.get_connection().unwrap();
        // throw away the result, just make sure it does not fail
        let _: () = con.set("key", "foobar").unwrap();
        let res: String = con.get("key").unwrap();
        println!("redis key value: {}", res);
        ...

        return true;
    }
}
```

`nslookup("redis", "").unwrap()[0].ip()` 表示寻找域名为redis的ip地址，我们可以在启动APISIX的时候挂载redis进去，也可以直接使用ip跳过这个步骤

## 测试

启动redis容器 `docker run -d --name redis -p 6379:6379 redis`

启动APISIX时 `--link` redis容器
```
docker run -d --name apisix \
-v `pwd`/apisix/config.yaml:/usr/local/apisix/conf/config.yaml \
-v `pwd`/target/wasm32-wasi/debug/heimdall.wasm:/usr/local/apisix/apisix/plugins/heimdall.wasm \
-v `pwd`/apisix/apisix.yaml:/usr/local/apisix/conf/apisix.yaml \
--link redis:redis \
-p 9080:9080 charlesvhe/apisix:wasm
```

http://localhost:9080/httpbin/get

可以看到几个重要的信息：
```
...
2022-11-30 14:20:07 nslookup redis: 172.17.0.2
2022-11-30 14:20:07 redis key value: foobar
...
```

WasmEdge官方已经改造常用重要库并推送到中央仓库，替换依赖名即可：

https://lib.rs/crates/tokio_wasi

https://lib.rs/crates/hyper_wasi

更多库参考 https://github.com/WasmEdge

其他官方未改造推送中央库的都可以按照上述方式进行改造
