FROM apache/apisix

COPY "./apisix-base_1.0.0-0~debianbullseye-slim_amd64.deb" /apisix-base.deb
RUN dpkg -i /apisix-base.deb

RUN sed -i 's/wasm_vm wasmtime/wasm_vm wasmedge/' /usr/local/apisix/apisix/cli/ngx_tpl.lua
