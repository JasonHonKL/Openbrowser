// bootstrap.js — Sets up document, window, Element, fetch for pardus-browser JS runtime.

// ==================== Event System ====================

// Event listener storage (global)
const _eventListeners = new Map();
// Timer callback storage
const _timerCallbacks = new Map();

class Event {
  constructor(type, eventInitDict = {}) {
    this.type = type;
    this.bubbles = eventInitDict.bubbles || false;
    this.cancelable = eventInitDict.cancelable || false;
    this.composed = eventInitDict.composed || false;
    this.detail = eventInitDict.detail || null;
    this.timeStamp = Date.now();
    this.defaultPrevented = false;
    this.propagationStopped = false;
    this.immediatePropagationStopped = false;
    this.target = null;
    this.currentTarget = null;
    this.eventPhase = 0; // NONE: 0, CAPTURING: 1, AT_TARGET: 2, BUBBLING: 3
  }

  preventDefault() {
    if (this.cancelable) {
      this.defaultPrevented = true;
    }
  }

  stopPropagation() {
    this.propagationStopped = true;
  }

  stopImmediatePropagation() {
    this.immediatePropagationStopped = true;
    this.propagationStopped = true;
  }

  initEvent(type, bubbles, cancelable) {
    this.type = type;
    this.bubbles = bubbles;
    this.cancelable = cancelable;
  }
}

class CustomEvent extends Event {
  constructor(type, eventInitDict = {}) {
    super(type, eventInitDict);
    this.detail = eventInitDict.detail || null;
  }
}

// Event phases
Event.NONE = 0;
Event.CAPTURING_PHASE = 1;
Event.AT_TARGET = 2;
Event.BUBBLING_PHASE = 3;

// Helper: Get or create listener array for a node
function _getListeners(nodeId, eventType) {
  if (!_eventListeners.has(nodeId)) {
    _eventListeners.set(nodeId, new Map());
  }
  const nodeListeners = _eventListeners.get(nodeId);
  if (!nodeListeners.has(eventType)) {
    nodeListeners.set(eventType, []);
  }
  return nodeListeners.get(eventType);
}

// Helper: Dispatch event through the DOM tree
function _dispatchEventThroughTree(nodeId, event, phase) {
  const listeners = _getListeners(nodeId, event.type);
  const element = new Element(nodeId);

  event.currentTarget = element;
  event.eventPhase = phase;

  for (const listener of listeners) {
    if (event.immediatePropagationStopped) break;

    try {
      if (typeof listener.callback === 'function') {
        listener.callback.call(element, event);
      } else if (listener.callback && typeof listener.callback.handleEvent === 'function') {
        listener.callback.handleEvent(event);
      }
    } catch (e) {
      // Ignore errors in event handlers
    }
  }
}

// ==================== Element wrapper ====================

class Element {
  constructor(nodeId) {
    this.__nodeId = nodeId;
  }

  // ---- Properties ----
  get tagName() { return Deno.core.ops.op_get_tag_name(this.__nodeId); }
  get id() { return Deno.core.ops.op_get_node_id_attr(this.__nodeId); }
  set id(v) { Deno.core.ops.op_set_node_id_attr(this.__nodeId, v); }
  get className() { return Deno.core.ops.op_get_class_name(this.__nodeId); }
  set className(v) { Deno.core.ops.op_set_class_name(this.__nodeId, v); }
  get innerHTML() { return Deno.core.ops.op_get_inner_html(this.__nodeId); }
  set innerHTML(v) { Deno.core.ops.op_set_inner_html(this.__nodeId, v); }
  get textContent() { return Deno.core.ops.op_get_text_content(this.__nodeId); }
  set textContent(v) { Deno.core.ops.op_set_text_content(this.__nodeId, v); }
  get outerHTML() { return Deno.core.ops.op_get_inner_html(this.__nodeId); }

  get children() {
    return Deno.core.ops.op_get_children(this.__nodeId).map(id => new Element(id));
  }

  get childElementCount() { return Deno.core.ops.op_get_children(this.__nodeId).length; }

  get firstChild() {
    const ids = Deno.core.ops.op_get_children(this.__nodeId);
    return ids.length > 0 ? new Element(ids[0]) : null;
  }

  get lastChild() {
    const ids = Deno.core.ops.op_get_children(this.__nodeId);
    return ids.length > 0 ? new Element(ids[ids.length - 1]) : null;
  }

  get parentElement() {
    const pid = Deno.core.ops.op_get_parent(this.__nodeId);
    return pid ? new Element(pid) : null;
  }

  get nextSibling() {
    const pid = Deno.core.ops.op_get_parent(this.__nodeId);
    if (!pid) return null;
    const siblings = Deno.core.ops.op_get_children(pid);
    const idx = siblings.indexOf(this.__nodeId);
    return idx >= 0 && idx < siblings.length - 1 ? new Element(siblings[idx + 1]) : null;
  }

  get previousSibling() {
    const sid = Deno.core.ops.op_get_previous_sibling(this.__nodeId);
    return sid ? new Element(sid) : null;
  }

  get nodeType() { return Deno.core.ops.op_get_node_type(this.__nodeId); }
  get nodeName() { return Deno.core.ops.op_get_node_name(this.__nodeId); }

  // ---- DOM Manipulation ----
  appendChild(child) {
    Deno.core.ops.op_append_child(this.__nodeId, child.__nodeId);
    return child;
  }

  removeChild(child) {
    Deno.core.ops.op_remove_child(this.__nodeId, child.__nodeId);
    return child;
  }

  insertBefore(newNode, refNode) {
    const refId = refNode ? refNode.__nodeId : 0;
    Deno.core.ops.op_insert_before(this.__nodeId, newNode.__nodeId, refId);
    return newNode;
  }

  replaceChild(newChild, oldChild) {
    Deno.core.ops.op_replace_child(this.__nodeId, newChild.__nodeId, oldChild.__nodeId);
    return oldChild;
  }

  cloneNode(deep = false) {
    const newId = Deno.core.ops.op_clone_node(this.__nodeId, deep);
    return new Element(newId);
  }

  // ---- Attributes ----
  setAttribute(name, value) {
    Deno.core.ops.op_set_attribute(this.__nodeId, name, String(value));
  }

  getAttribute(name) {
    return Deno.core.ops.op_get_attribute(this.__nodeId, name);
  }

  removeAttribute(name) {
    Deno.core.ops.op_remove_attribute(this.__nodeId, name);
  }

  hasAttribute(name) {
    return Deno.core.ops.op_get_attribute(this.__nodeId, name) !== null;
  }

  hasAttributes() {
    return Deno.core.ops.op_has_attributes(this.__nodeId);
  }

  getAttributeNames() {
    return Deno.core.ops.op_get_attribute_names(this.__nodeId);
  }

  // ---- Query Selectors ----
  querySelector(selector) {
    const id = Deno.core.ops.op_query_selector(this.__nodeId, selector);
    return id ? new Element(id) : null;
  }

  querySelectorAll(selector) {
    return Deno.core.ops.op_query_selector_all(this.__nodeId, selector).map(id => new Element(id));
  }

  // ---- Event Handling ----
  addEventListener(type, callback, options = {}) {
    const capture = typeof options === 'boolean' ? options : (options.capture || false);
    const listeners = _getListeners(this.__nodeId, type);

    // Check for duplicate
    const exists = listeners.some(l => l.callback === callback && l.capture === capture);
    if (!exists) {
      listeners.push({ callback, capture, once: options.once || false });
    }
  }

  removeEventListener(type, callback, options = {}) {
    const capture = typeof options === 'boolean' ? options : (options.capture || false);
    const listeners = _getListeners(this.__nodeId, type);
    const idx = listeners.findIndex(l => l.callback === callback && l.capture === capture);
    if (idx >= 0) {
      listeners.splice(idx, 1);
    }
  }

  dispatchEvent(event) {
    event.target = this;

    // Build propagation path (simplified - just parent chain)
    const path = [];
    let current = this.__nodeId;
    while (current) {
      path.unshift(current);
      current = Deno.core.ops.op_get_parent(current);
      if (!current) break;
    }

    // Capturing phase
    event.eventPhase = Event.CAPTURING_PHASE;
    for (const nodeId of path.slice(0, -1)) {
      if (event.propagationStopped) break;
      _dispatchEventThroughTree(nodeId, event, Event.CAPTURING_PHASE);
    }

    // At target
    if (!event.propagationStopped) {
      event.eventPhase = Event.AT_TARGET;
      _dispatchEventThroughTree(this.__nodeId, event, Event.AT_TARGET);
    }

    // Bubbling phase
    if (event.bubbles && !event.propagationStopped) {
      event.eventPhase = Event.BUBBLING_PHASE;
      for (const nodeId of path.slice(0, -1).reverse()) {
        if (event.propagationStopped) break;
        _dispatchEventThroughTree(nodeId, event, Event.BUBBLING_PHASE);
      }
    }

    event.eventPhase = Event.NONE;
    return !event.defaultPrevented;
  }

  // ---- Utility Methods ----
  contains(other) {
    if (!other) return false;
    return Deno.core.ops.op_contains(this.__nodeId, other.__nodeId);
  }

  hasChildNodes() {
    return Deno.core.ops.op_has_child_nodes(this.__nodeId);
  }

  // ---- Style ----
  get style() {
    const nodeId = this.__nodeId;
    return new Proxy({}, {
      set(_, prop, value) {
        Deno.core.ops.op_set_style(nodeId, prop, String(value));
        return true;
      },
      get(_, prop) {
        // Return empty string for style property reads
        return '';
      }
    });
  }

  // ---- Class List (simplified) ----
  get classList() {
    const nodeId = this.__nodeId;
    const self = this;
    return {
      add(...tokens) {
        const current = self.className.split(/\s+/).filter(s => s);
        for (const token of tokens) {
          if (!current.includes(token)) {
            current.push(token);
          }
        }
        self.className = current.join(' ');
      },
      remove(...tokens) {
        const current = self.className.split(/\s+/).filter(s => s);
        for (const token of tokens) {
          const idx = current.indexOf(token);
          if (idx >= 0) current.splice(idx, 1);
        }
        self.className = current.join(' ');
      },
      toggle(token, force) {
        const current = self.className.split(/\s+/).filter(s => s);
        const has = current.includes(token);
        if (force === true || (!has && force !== false)) {
          if (!has) current.push(token);
          self.className = current.join(' ');
          return true;
        } else if (force === false || has) {
          const idx = current.indexOf(token);
          if (idx >= 0) current.splice(idx, 1);
          self.className = current.join(' ');
          return false;
        }
        return has;
      },
      contains(token) {
        return self.className.split(/\s+/).includes(token);
      },
      get length() {
        return self.className.split(/\s+/).filter(s => s).length;
      }
    };
  }

  // ---- Dataset ----
  get dataset() {
    const nodeId = this.__nodeId;
    return new Proxy({}, {
      set(_, prop, value) {
        const attrName = 'data-' + prop.replace(/([A-Z])/g, '-$1').toLowerCase();
        Deno.core.ops.op_set_attribute(nodeId, attrName, String(value));
        return true;
      },
      get(_, prop) {
        const attrName = 'data-' + prop.replace(/([A-Z])/g, '-$1').toLowerCase();
        return Deno.core.ops.op_get_attribute(nodeId, attrName) || undefined;
      }
    });
  }

  // ---- Convenience Methods ----
  focus() { /* no-op for headless */ }
  blur() { /* no-op for headless */ }
  click() {
    const event = new Event('click', { bubbles: true, cancelable: true });
    this.dispatchEvent(event);
  }
}

// ==================== MutationObserver ====================

class MutationObserver {
  constructor(callback) {
    this.__callback = callback;
    this.__id = Deno.core.ops.op_register_observer(0, true, true, false);
  }

  observe(target, options = {}) {
    if (target && target.__nodeId) {
      this.__id = Deno.core.ops.op_register_observer(
        target.__nodeId,
        options.childList || false,
        options.attributes !== false,
        options.subtree || false
      );
    }
  }

  disconnect() {
    Deno.core.ops.op_disconnect_observer(this.__id);
  }

  takeRecords() {
    return Deno.core.ops.op_take_mutation_records().map(r => ({
      type: r.type_,
      target: r.target ? new Element(r.target) : null,
      addedNodes: (r.added_nodes || []).map(id => new Element(id)),
      removedNodes: (r.removed_nodes || []).map(id => new Element(id)),
      attributeName: r.attribute_name || null,
      oldValue: r.old_value || null,
    }));
  }
}

// ==================== TextNode wrapper ====================

class TextNode {
  constructor(nodeId) {
    this.__nodeId = nodeId;
  }
  get textContent() { return Deno.core.ops.op_get_text_content(this.__nodeId); }
  set textContent(v) { Deno.core.ops.op_set_text_content(this.__nodeId, v); }
  get nodeType() { return 3; }
  get nodeName() { return '#text'; }
  get parentElement() {
    const pid = Deno.core.ops.op_get_parent(this.__nodeId);
    return pid ? new Element(pid) : null;
  }
}

// ==================== DocumentFragment wrapper ====================

class DocumentFragment {
  constructor(nodeId) {
    this.__nodeId = nodeId;
  }
  appendChild(child) {
    Deno.core.ops.op_append_child(this.__nodeId, child.__nodeId);
    return child;
  }
  get children() {
    return Deno.core.ops.op_get_children(this.__nodeId).map(id => new Element(id));
  }
  get nodeType() { return 11; }
  get nodeName() { return '#document-fragment'; }
  querySelector(selector) {
    const id = Deno.core.ops.op_query_selector(this.__nodeId, selector);
    return id ? new Element(id) : null;
  }
  querySelectorAll(selector) {
    return Deno.core.ops.op_query_selector_all(this.__nodeId, selector).map(id => new Element(id));
  }
}

// ==================== DOMContentLoaded ====================

let _DOMContentLoadedFired = false;
const _DOMContentLoadedListeners = [];

function _fireDOMContentLoaded() {
    _DOMContentLoadedFired = true;
    for (const listener of _DOMContentLoadedListeners) {
        try { listener.callback(new Event('DOMContentLoaded')); } catch (e) {}
    }
    _DOMContentLoadedListeners.length = 0;
}

// ==================== Document object ====================

const document = {
  createElement(tag) { return new Element(Deno.core.ops.op_create_element(tag)); },
  createTextNode(text) { return new TextNode(Deno.core.ops.op_create_text_node(text)); },
  createDocumentFragment() { return new DocumentFragment(Deno.core.ops.op_create_document_fragment()); },
  getElementById(id) {
    const nid = Deno.core.ops.op_get_element_by_id(id);
    return nid ? new Element(nid) : null;
  },
  querySelector(selector) {
    const nid = Deno.core.ops.op_query_selector(0, selector);
    return nid ? new Element(nid) : null;
  },
  querySelectorAll(selector) {
    return Deno.core.ops.op_query_selector_all(0, selector).map(id => new Element(id));
  },
  get documentElement() { return new Element(Deno.core.ops.op_get_document_element()); },
  get head() { return new Element(Deno.core.ops.op_get_head()); },
  get body() { return new Element(Deno.core.ops.op_get_body()); },

  // Event handling
  addEventListener(type, callback, options) {
    if (type === 'DOMContentLoaded') {
      _DOMContentLoadedListeners.push({ callback, options });
      if (_DOMContentLoadedFired) {
        try { callback(new Event('DOMContentLoaded')); } catch (e) {}
      }
      return;
    }
    const docEl = this.documentElement;
    if (docEl) {
      docEl.addEventListener(type, callback, options);
    }
  },

  removeEventListener(type, callback, options) {
    if (type === 'DOMContentLoaded') {
      const idx = _DOMContentLoadedListeners.findIndex(l => l.callback === callback);
      if (idx >= 0) _DOMContentLoadedListeners.splice(idx, 1);
      return;
    }
    const docEl = this.documentElement;
    if (docEl) {
      docEl.removeEventListener(type, callback, options);
    }
  },

  dispatchEvent(event) {
    const docEl = this.documentElement;
    if (docEl) {
      return docEl.dispatchEvent(event);
    }
    return true;
  },

  createEvent(type) {
    const eventClasses = {
      'Event': Event,
      'CustomEvent': CustomEvent,
      'UIEvent': Event,
      'MouseEvent': Event,
      'KeyboardEvent': Event,
    };
    const EventClass = eventClasses[type] || Event;
    const event = new EventClass('');
    return event;
  }
};

// ==================== Fetch polyfill ====================

async function fetch(input, init) {
  init = init || {};
  const url = typeof input === "string" ? input : (input.url || String(input));
  const method = init.method || "GET";
  const headers = {};
  if (init.headers) {
    if (init.headers instanceof Map) {
      init.headers.forEach((v, k) => headers[k] = v);
    } else if (typeof init.headers === "object") {
      Object.assign(headers, init.headers);
    }
  }
  const resp = await Deno.core.ops.op_fetch({
    url,
    method,
    headers,
    body: init.body || null
  });

  return {
    ok: resp.ok,
    status: resp.status,
    statusText: resp.status_text,
    url,
    headers: new Map(Object.entries(resp.headers || {})),
    text: async () => resp.body,
    json: async () => JSON.parse(resp.body),
  };
}

// ==================== Window object ====================

const window = {
  document,
  fetch,
  addEventListener: document.addEventListener.bind(document),
  removeEventListener: document.removeEventListener.bind(document),
  location: new Proxy({
    href: "",
    origin: "",
    protocol: "https:",
    host: "",
    hostname: "",
    pathname: "/",
    search: "",
    hash: "",
    assign: function(url) {
      var docEl = document.documentElement;
      if (docEl) docEl.setAttribute('data-pardus-navigation-href', String(url));
    },
    replace: function(url) {
      var docEl = document.documentElement;
      if (docEl) docEl.setAttribute('data-pardus-navigation-href', String(url));
    },
    reload: function() {}
  }, {
    set(target, prop, value) {
      target[prop] = value;
      // Detect navigation via window.location.href = '/url'
      if (prop === 'href') {
        var docEl = document.documentElement;
        if (docEl) {
          docEl.setAttribute('data-pardus-navigation-href', String(value));
        }
      }
      return true;
    }
  }),
  navigator: {
    userAgent: typeof globalThis.__pardusUserAgent !== 'undefined'
        ? globalThis.__pardusUserAgent
        : "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    appName: "Netscape",
    appVersion: "5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    appCodeName: "Mozilla",
    product: "Gecko",
    productSub: "20030107",
    vendor: "Google Inc.",
    vendorSub: "",
    platform: "MacIntel",
    language: "en-US",
    languages: ["en-US", "en"],
    onLine: true,
    cookieEnabled: true,
    doNotTrack: null,
    hardwareConcurrency: 8,
    maxTouchPoints: 0,
    pdfViewerEnabled: true,
    webdriver: false,
    sendBeacon: function() { return true; },
    getBattery: function() {
      return Promise.resolve({
        charging: true, chargingTime: 0, dischargingTime: Infinity, level: 1.0,
        addEventListener: function() {}, removeEventListener: function() {}
      });
    },
    getGamepads: function() { return []; },
    javaEnabled: function() { return false; },
    plugins: (function() {
      function FakePlugin(name, desc, fn) {
        this.name = name; this.description = desc; this.filename = fn; this.length = 0;
      }
      FakePlugin.prototype.item = function() { return null; };
      FakePlugin.prototype.namedItem = function() { return null; };
      var arr = [
        new FakePlugin("PDF Viewer", "Portable Document Format", "internal-pdf-viewer"),
        new FakePlugin("Chrome PDF Viewer", "Portable Document Format", "internal-pdf-viewer"),
        new FakePlugin("Chromium PDF Viewer", "Portable Document Format", "internal-pdf-viewer"),
      ];
      arr.item = function(i) { return this[i] || null; };
      arr.namedItem = function(n) { for (var j = 0; j < this.length; j++) { if (this[j].name === n) return this[j]; } return null; };
      arr.refresh = function() {};
      return arr;
    })(),
    mimeTypes: (function() {
      var arr = [{ type: "application/pdf", suffixes: "pdf", description: "Portable Document Format" }];
      arr.item = function(i) { return this[i] || null; };
      arr.namedItem = function() { return null; };
      return arr;
    })(),
    connection: { effectiveType: "4g", rtt: 50, downlink: 10, saveData: false, addEventListener: function() {}, removeEventListener: function() {} },
    storage: { estimate: function() { return Promise.resolve({ quota: 299706064076, usage: 0 }); } },
    clipboard: { readText: function() { return Promise.resolve(""); }, writeText: function() { return Promise.resolve(); } },
    permissions: { query: function() { return Promise.resolve({ state: "granted", addEventListener: function() {} }); } },
    mediaDevices: { enumerateDevices: function() { return Promise.resolve([]); } },
    locks: { request: function(n, cb) { return Promise.resolve(typeof cb === 'function' ? cb() : undefined); } },
    credentials: { get: function() { return Promise.resolve(null); }, create: function() { return Promise.resolve(null); } },
    uaData: {
      brands: [ { brand: "Google Chrome", version: "131" }, { brand: "Chromium", version: "131" }, { brand: "Not_A Brand", version: "24" } ],
      mobile: false,
      platform: "macOS",
      getHighEntropyValues: function() {
        return Promise.resolve({ architecture: "arm", bitness: "64", model: "", platformVersion: "14.0.0",
          fullVersionList: [ { brand: "Google Chrome", version: "131.0.6778.86" }, { brand: "Chromium", version: "131.0.6778.86" } ]
        });
      },
      toJSON: function() { return { brands: this.brands, mobile: this.mobile, platform: this.platform }; }
    },
  },
  console: {
    log(...a) {},
    warn(...a) {},
    error(...a) {},
    info(...a) {},
    debug(...a) {},
  },
  setTimeout(fn, ms) {
    if (typeof fn === 'function') {
      // Store callback in a map and pass its body to the timer op
      var id = Deno.core.ops.op_set_timeout(fn.toString(), ms || 0);
      _timerCallbacks.set(id, fn);
      return id;
    }
    return 0;
  },
  setInterval(fn, ms) {
    if (typeof fn === 'function') {
      var id = Deno.core.ops.op_set_interval(fn.toString(), ms || 0);
      _timerCallbacks.set(id, fn);
      return id;
    }
    return 0;
  },
  clearTimeout(id) {
    _timerCallbacks.delete(id);
    Deno.core.ops.op_clear_timer(id);
  },
  clearInterval(id) {
    _timerCallbacks.delete(id);
    Deno.core.ops.op_clear_timer(id);
  },
  getComputedStyle() { return new Proxy({}, { get: () => "" }); },
  matchMedia() {
    return { matches: false, addListener() {}, removeListener() {} };
  },
  innerWidth: 1280,
  innerHeight: 720,
  dispatchEvent(event) {
    return document.dispatchEvent(event);
  },
  Event,
  CustomEvent,
};

// ==================== EventSource / SSE ====================

const __sseInstances = new Map();
const __sseCallbacks = new Map();

function __sse_dispatch(id, eventType, eventInit, readyState) {
  const es = __sseInstances.get(id);
  if (!es) return;
  const cb = __sseCallbacks.get(id);
  if (!cb) return;

  if (readyState !== undefined) {
    es.readyState = readyState;
  }

  const event = new MessageEvent(eventType, eventInit);

  const listeners = (cb.listeners && cb.listeners[eventType]) || [];
  for (const fn of listeners) {
    try { fn.call(es, event); } catch (e) {}
  }

  if (eventType === 'open' && cb.onopen) {
    try { cb.onopen.call(es, event); } catch (e) {}
  }
  if (eventType === 'message' && cb.onmessage) {
    try { cb.onmessage.call(es, event); } catch (e) {}
  }
  if (eventType === 'error' && cb.onerror) {
    try { cb.onerror.call(es, event); } catch (e) {}
  }
}

class MessageEvent {
  constructor(type, eventInitDict) {
    eventInitDict = eventInitDict || {};
    this.type = type;
    this.data = eventInitDict.data !== undefined ? eventInitDict.data : null;
    this.origin = eventInitDict.origin || '';
    this.lastEventId = eventInitDict.lastEventId || '';
    this.bubbles = false;
    this.cancelable = false;
    this.composed = false;
  }
}

class EventSource {
  constructor(url) {
    const absoluteUrl = typeof URL !== 'undefined'
      ? new URL(url, globalThis.window ? globalThis.window.location.href : url).href
      : url;
    this.url = absoluteUrl;
    this.readyState = EventSource.CONNECTING;
    this.__id = Deno.core.ops.op_sse_open(absoluteUrl);
    __sseInstances.set(this.__id, this);
    __sseCallbacks.set(this.__id, {
      onopen: null,
      onmessage: null,
      onerror: null,
      listeners: {}
    });
  }

  get onopen() {
    const cb = __sseCallbacks.get(this.__id);
    return cb ? cb.onopen : null;
  }
  set onopen(fn) {
    const cb = __sseCallbacks.get(this.__id);
    if (cb) cb.onopen = typeof fn === 'function' ? fn : null;
  }

  get onmessage() {
    const cb = __sseCallbacks.get(this.__id);
    return cb ? cb.onmessage : null;
  }
  set onmessage(fn) {
    const cb = __sseCallbacks.get(this.__id);
    if (cb) cb.onmessage = typeof fn === 'function' ? fn : null;
  }

  get onerror() {
    const cb = __sseCallbacks.get(this.__id);
    return cb ? cb.onerror : null;
  }
  set onerror(fn) {
    const cb = __sseCallbacks.get(this.__id);
    if (cb) cb.onerror = typeof fn === 'function' ? fn : null;
  }

  addEventListener(type, callback) {
    const cb = __sseCallbacks.get(this.__id);
    if (!cb) return;
    if (!cb.listeners[type]) cb.listeners[type] = [];
    if (!cb.listeners[type].includes(callback)) {
      cb.listeners[type].push(callback);
    }
  }

  removeEventListener(type, callback) {
    const cb = __sseCallbacks.get(this.__id);
    if (!cb || !cb.listeners[type]) return;
    const idx = cb.listeners[type].indexOf(callback);
    if (idx >= 0) cb.listeners[type].splice(idx, 1);
  }

  close() {
    Deno.core.ops.op_sse_close(this.__id);
    this.readyState = EventSource.CLOSED;
    __sseInstances.delete(this.__id);
    __sseCallbacks.delete(this.__id);
  }

  static get CONNECTING() { return 0; }
  static get OPEN() { return 1; }
  static get CLOSED() { return 2; }
}

// ==================== Globals ====================

globalThis.window = window;
globalThis.document = document;
globalThis.fetch = fetch;
globalThis.Element = Element;
globalThis.TextNode = TextNode;
globalThis.DocumentFragment = DocumentFragment;
globalThis.Event = Event;
globalThis.CustomEvent = CustomEvent;
globalThis.MutationObserver = MutationObserver;
globalThis.MessageEvent = MessageEvent;
globalThis.EventSource = EventSource;
globalThis.Node = {
  ELEMENT_NODE: 1,
  TEXT_NODE: 3,
  DOCUMENT_FRAGMENT_NODE: 11,
  DOCUMENT_NODE: 9
};
globalThis.setTimeout = window.setTimeout;
globalThis.setInterval = window.setInterval;
globalThis.clearTimeout = window.clearTimeout;
globalThis.clearInterval = window.clearInterval;
globalThis.console = window.console;
globalThis.navigator = window.navigator;
globalThis.performance = (function() {
  var _origin = Date.now();
  var _timing = {
    navigationStart: _origin - 500,
    unloadEventStart: 0, unloadEventEnd: 0,
    redirectStart: 0, redirectEnd: 0,
    fetchStart: _origin - 490,
    domainLookupStart: _origin - 480, domainLookupEnd: _origin - 470,
    connectStart: _origin - 470, connectEnd: _origin - 450,
    secureConnectionStart: _origin - 460,
    requestStart: _origin - 440,
    responseStart: _origin - 200, responseEnd: _origin - 100,
    domLoading: _origin - 90, domInteractive: _origin - 50,
    domContentLoadedEventStart: _origin - 40, domContentLoadedEventEnd: _origin - 30,
    domComplete: _origin - 10,
    loadEventStart: _origin - 5, loadEventEnd: _origin,
  };
  return {
    now: function() { return Date.now() - _origin; },
    timeOrigin: _origin,
    timing: _timing,
    navigation: { type: 0, redirectCount: 0 },
    getEntries: function() { return []; },
    getEntriesByType: function() { return []; },
    getEntriesByName: function() { return []; },
    mark: function() {}, measure: function() {},
    clearMarks: function() {}, clearMeasures: function() {},
    toJSON: function() { return { timing: _timing, navigation: this.navigation }; }
  };
})();
globalThis.self = globalThis;
globalThis.top = globalThis;
globalThis.parent = globalThis;
globalThis.frames = globalThis;
globalThis.__modules = {};

// ==================== window.chrome (Chrome-specific) ====================
globalThis.chrome = {
  runtime: {
    onMessage: { addListener: function() {}, removeListener: function() {} },
    onConnect: { addListener: function() {}, removeListener: function() {} },
    sendMessage: function() {},
    connect: function() { return { onMessage: { addListener: function() {} }, postMessage: function() {}, disconnect: function() {} }; },
    getURL: function(p) { return "chrome-extension://invalid/" + p; },
    id: undefined
  },
  csi: function() { return { startE: Date.now(), onloadT: Date.now(), pageT: 0 }; },
  loadTimes: function() {
    var t = Date.now() / 1000;
    return { requestTime: t, startLoadTime: t, commitLoadTime: t, finishDocumentLoadTime: t, finishLoadTime: t,
      firstPaintTime: t, firstPaintAfterLoadTime: 0, navigationType: "Other", wasFetchedViaSpdy: true,
      wasNpnNegotiated: true, npnNegotiatedProtocol: "h2", wasAlternateProtocolAvailable: false, connectionInfo: "h2" };
  },
};

// ==================== Screen object ====================
globalThis.screen = {
  width: 1920, height: 1080,
  availWidth: 1920, availHeight: 1055,
  colorDepth: 30, pixelDepth: 30,
  orientation: { angle: 0, type: "landscape-primary", addEventListener: function() {}, removeEventListener: function() {} }
};

// ==================== Window dimension overrides ====================
Object.defineProperty(window, 'outerWidth', { value: 1920, writable: true });
Object.defineProperty(window, 'outerHeight', { value: 1055, writable: true });
Object.defineProperty(window, 'screenX', { value: 0, writable: true });
Object.defineProperty(window, 'screenY', { value: 25, writable: true });
Object.defineProperty(window, 'screenLeft', { value: 0, writable: true });
Object.defineProperty(window, 'screenTop', { value: 25, writable: true });
Object.defineProperty(window, 'devicePixelRatio', { value: 2, writable: true });
Object.defineProperty(window, 'pageXOffset', { get: function() { return 0; } });
Object.defineProperty(window, 'pageYOffset', { get: function() { return 0; } });
Object.defineProperty(window, 'scrollX', { get: function() { return 0; } });
Object.defineProperty(window, 'scrollY', { get: function() { return 0; } });

// ==================== Toolbar stubs ====================
window.locationbar = { visible: true };
window.menubar = { visible: true };
window.personalbar = { visible: true };
window.scrollbars = { visible: true };
window.statusbar = { visible: true };
window.toolbar = { visible: true };

// ==================== History stub ====================
globalThis.history = {
  length: 1, scrollRestoration: "auto", state: null,
  back: function() {}, forward: function() {}, go: function() {},
  pushState: function() {}, replaceState: function() {},
};

// ==================== Additional Web APIs ====================

// trustedTypes — stub that satisfies Google's policy check
globalThis.trustedTypes = {
  createPolicy: function(name, rules) {
    return {
      createHTML: rules && rules.createHTML ? rules.createHTML : (s) => s,
      createScript: rules && rules.createScript ? rules.createScript : (s) => s,
      createScriptURL: rules && rules.createScriptURL ? rules.createScriptURL : (s) => s,
    };
  },
  isHTML: function() { return false; },
  isScript: function() { return false; },
  isScriptURL: function() { return false; },
};

// requestAnimationFrame — execute callback immediately
globalThis.requestAnimationFrame = function(cb) {
  if (typeof cb === 'function') {
    try { cb(Date.now()); } catch(e) {}
  }
  return 1;
};
globalThis.cancelAnimationFrame = function() {};

// btoa / atob — Base64 encoding/decoding
globalThis.btoa = function(str) {
  var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
  var output = '';
  for (var i = 0; i < str.length; i += 3) {
    var b1 = str.charCodeAt(i);
    var b2 = i + 1 < str.length ? str.charCodeAt(i + 1) : 0;
    var b3 = i + 2 < str.length ? str.charCodeAt(i + 2) : 0;
    output += chars[b1 >> 2] + chars[((b1 & 3) << 4) | (b2 >> 4)];
    output += i + 1 < str.length ? chars[((b2 & 15) << 2) | (b3 >> 6)] : '=';
    output += i + 2 < str.length ? chars[b3 & 63] : '=';
  }
  return output;
};
globalThis.atob = function(b64) {
  var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
  var output = '';
  var i = 0;
  b64 = b64.replace(/[^A-Za-z0-9\+\/\=]/g, '');
  while (i < b64.length) {
    var e1 = chars.indexOf(b64.charAt(i++));
    var e2 = chars.indexOf(b64.charAt(i++));
    var e3 = chars.indexOf(b64.charAt(i++));
    var e4 = chars.indexOf(b64.charAt(i++));
    output += String.fromCharCode((e1 << 2) | (e2 >> 4));
    if (e3 !== 64) output += String.fromCharCode(((e2 & 15) << 4) | (e3 >> 2));
    if (e4 !== 64) output += String.fromCharCode(((e3 & 3) << 6) | e4);
  }
  return output;
};

// TextEncoder / TextDecoder stubs
if (typeof TextEncoder === 'undefined') {
  globalThis.TextEncoder = function() {
    this.encode = function(str) { return new Uint8Array([]); };
  };
}
if (typeof TextDecoder === 'undefined') {
  globalThis.TextDecoder = function() {
    this.decode = function(buf) { return ''; };
  };
}

// URL constructor (if not already available)
if (typeof URL === 'undefined') {
  globalThis.URL = function(url, base) {
    // Minimal URL parser
    this.href = url;
    this.origin = '';
    this.protocol = '';
    this.host = '';
    this.hostname = '';
    this.pathname = '';
    this.search = '';
    this.hash = '';
  };
}

// XMLHttpRequest stub — enough to not crash scripts
globalThis.XMLHttpRequest = function() {
  this.readyState = 0;
  this.status = 0;
  this.responseText = '';
  this.responseURL = '';
  this.onreadystatechange = null;
  this.onload = null;
  this.onerror = null;
  this.open = function() { this.readyState = 1; };
  this.send = function() { this.readyState = 4; this.status = 200; if (this.onload) this.onload(); };
  this.setRequestHeader = function() {};
  this.getResponseHeader = function() { return null; };
  this.abort = function() {};
};

// Image stub
globalThis.Image = function() {
  this.src = '';
  this.onload = null;
  this.onerror = null;
};

// Promise-based queueMicrotask
if (typeof globalThis.queueMicrotask === 'undefined') {
  globalThis.queueMicrotask = function(cb) {
    Promise.resolve().then(cb);
  };
}
