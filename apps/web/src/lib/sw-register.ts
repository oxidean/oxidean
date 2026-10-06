/**
 * Build the inline service-worker registration snippet.
 * Build id must already be sanitized ([a-zA-Z0-9._-] only) — never pass raw env.
 *
 * Registers with `?v=<buildId>` so each deploy is a distinct script URL, calls
 * `registration.update()`, activates waiting workers, and reloads once when
 * the new controller takes over — but only when a controller already existed.
 * On a fresh install `clients.claim()` hands the just-loaded page to the new
 * worker transparently: the document was fetched from the same build, so a
 * reload buys nothing — and it kills any tap that lands while the document
 * tears down (issue #111's intermittent dead mobile taps).
 */
export function buildSwRegisterScript(buildId: string): string {
  const id = buildId.replace(/[^a-zA-Z0-9._-]/g, "").slice(0, 32) || "unknown";
  // Keep as one expression string for dangerouslySetInnerHTML (T-03-19: no
  // user-controlled interpolation — buildId comes from Vite define only).
  return (
    "(function(){" +
    'if(!("serviceWorker" in navigator))return;' +
    "var BUILD=" +
    JSON.stringify(id) +
    ";" +
    'window.addEventListener("load",function(){' +
    'navigator.serviceWorker.register("/sw.js?v="+encodeURIComponent(BUILD)).then(function(reg){' +
    "function skip(w){if(w)w.postMessage({type:'SKIP_WAITING'});}" +
    "if(reg.waiting)skip(reg.waiting);" +
    'reg.addEventListener("updatefound",function(){' +
    "var w=reg.installing;if(!w)return;" +
    'w.addEventListener("statechange",function(){' +
    'if(w.state==="installed"&&navigator.serviceWorker.controller)skip(w);' +
    "});" +
    "});" +
    "return reg.update();" +
    "}).catch(function(){});" +
    "var refreshing=false,hadController=!!navigator.serviceWorker.controller;" +
    'navigator.serviceWorker.addEventListener("controllerchange",function(){' +
    "if(refreshing)return;" +
    "if(!hadController){hadController=true;return;}" +
    "refreshing=true;" +
    'try{var k="oxidean-sw-"+BUILD;if(sessionStorage.getItem(k))return;sessionStorage.setItem(k,"1");}catch(e){}' +
    "window.location.reload();" +
    "});" +
    "});" +
    "})();"
  );
}
