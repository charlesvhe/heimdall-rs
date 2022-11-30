FROM apache/apisix:3.0.0-centos

COPY "./apisix-base-1.0.0-0.el7.x86_64.rpm" /apisix-base.rpm
RUN rpm -vih --force /apisix-base.rpm

RUN sed -i 's/wasm_vm wasmtime/wasm_vm wasmedge/' /usr/local/apisix/apisix/cli/ngx_tpl.lua
