(() => {
  "use strict";
  const origin = __CALL_ORIGIN__;
  const jwtService = __JWT_SERVICE__;
  window.ipc.postMessage(JSON.stringify({ runtime_environment: {
    secure_context: window.isSecureContext,
    media_devices: !!navigator.mediaDevices,
    script_transform: typeof RTCRtpScriptTransform !== "undefined"
  }}));
  // No console forwarding. The only native IPC is a widget envelope.
  for (const key of ["log", "info", "debug", "warn", "error", "trace"]) console[key] = () => {};
  // WK custom schemes have an opaque DOM origin ("null"). WidgetApi's
  // same-window transport otherwise calls postMessage with that invalid target
  // origin. Intercept its envelopes directly; preserve ordinary postMessage.
  const nativePostMessage = window.postMessage.bind(window);
  window.postMessage = (data, ...args) => {
    if (data?.api === "fromWidget" || (data?.api === "toWidget" && "response" in data)) {
      window.ipc.postMessage(JSON.stringify(data));
      return;
    }
    return nativePostMessage(data, ...args);
  };
  Object.defineProperty(window, "__twaybladeReceive", { value: data => {
    window.dispatchEvent(new MessageEvent("message", { data, source: window, origin: new URL(origin).origin }));
  }});
  // Embedded MatrixRTC discovers foci without a Matrix HTTP connection. No
  // request to the homeserver leaves the runtime. This response is constructed
  // exclusively from the SDK's .well-known result in the parent process.
  const nativeFetch = window.fetch.bind(window);
  // Reachability only: no assertion/token, no body, no trust-store changes and
  // no TLS bypass. An HTTP error response still proves a trusted TLS session.
  nativeFetch(jwtService, { method: "HEAD", mode: "no-cors", credentials: "omit", cache: "no-store", signal: AbortSignal.timeout(5000) })
    .then(() => window.ipc.postMessage(JSON.stringify({ runtime_service_health: true })))
    .catch(() => window.ipc.postMessage(JSON.stringify({ runtime_service_health: false })));
  const serviceOrigin = new URL(jwtService).origin;
  const checkNetworkUrl = raw => {
    const url = new URL(raw, location.href);
    const httpsOrigin = url.origin.replace(/^wss:/, "https:");
    if (url.protocol === "twayblade-call:") return url;
    const credentials = [...url.searchParams.keys()].some(key => /token|authorization|password|secret|jwt/i.test(key));
    if (httpsOrigin !== serviceOrigin || url.username || url.password || credentials) {
      window.ipc.postMessage(JSON.stringify({ runtime_failure: "BridgeRejected" }));
      throw new DOMException("Call network policy rejected", "SecurityError");
    }
    return url;
  };
  window.fetch = (input, init) => {
    const raw = typeof input === "string" ? input : input.url;
    const url = new URL(raw, location.href);
    if (url.pathname === "/.well-known/matrix/client") {
      return Promise.resolve(new Response(JSON.stringify({
        "org.matrix.msc4143.rtc_foci": [{ type: "livekit", livekit_service_url: jwtService }]
      }), { headers: { "Content-Type": "application/json" } }));
    }
    checkNetworkUrl(raw);
    return nativeFetch(input, init);
  };
  const NativeWebSocket = window.WebSocket;
  window.WebSocket = class extends NativeWebSocket {
    constructor(url, protocols) {
      checkNetworkUrl(url);
      super(url, protocols);
    }
  };
  const nativeOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(method, url, ...args) {
    checkNetworkUrl(url);
    return nativeOpen.call(this, method, url, ...args);
  };
  for (const method of ["getUserMedia", "getDisplayMedia"]) {
    const devices = navigator.mediaDevices;
    if (typeof devices?.[method] !== "function") continue;
    const original = devices[method].bind(devices);
    devices[method] = async (...args) => {
      try { return await original(...args); }
      catch (error) {
        if (error.name === "NotAllowedError" || error.name === "PermissionDeniedError") {
          window.ipc.postMessage(JSON.stringify({ runtime_failure: "PermissionDenied" }));
        }
        throw error;
      }
    };
  }
})();
