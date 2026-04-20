// Stealth patches injected on every new document by BrowserFetcher when stealth is enabled.
// Hides Chromium automation artifacts to reduce bot detection fingerprinting.

// Hide navigator.webdriver
Object.defineProperty(navigator, 'webdriver', {
    get: () => undefined,
    configurable: true,
});

// Fake non-empty plugins list (headless Chrome has none)
if (!navigator.plugins.length) {
    Object.defineProperty(navigator, 'plugins', {
        get: () => {
            const arr = [1, 2, 3, 4, 5];
            arr.__proto__ = PluginArray.prototype;
            return arr;
        },
        configurable: true,
    });
    Object.defineProperty(navigator, 'mimeTypes', {
        get: () => {
            const arr = [1, 2, 3];
            arr.__proto__ = MimeTypeArray.prototype;
            return arr;
        },
        configurable: true,
    });
}

// Add a chrome object stub (expected by bot-detection scripts)
if (!window.chrome) {
    Object.defineProperty(window, 'chrome', {
        value: {
            app: { isInstalled: false },
            webstore: { onInstallStageChanged: {}, onDownloadProgress: {} },
            runtime: {
                PlatformOs: { MAC: 'mac', WIN: 'win', ANDROID: 'android', CROS: 'cros', LINUX: 'linux', OPENBSD: 'openbsd' },
                PlatformArch: { ARM: 'arm', ARM64: 'arm64', X86_32: 'x86-32', X86_64: 'x86-64', MIPS: 'mips', MIPS64: 'mips64' },
                RequestUpdateCheckStatus: { THROTTLED: 'throttled', NO_UPDATE: 'no_update', UPDATE_AVAILABLE: 'update_available' },
                OnInstalledReason: { INSTALL: 'install', UPDATE: 'update', CHROME_UPDATE: 'chrome_update', SHARED_MODULE_UPDATE: 'shared_module_update' },
                OnRestartRequiredReason: { APP_UPDATE: 'app_update', OS_UPDATE: 'os_update', PERIODIC: 'periodic' },
            },
        },
        writable: false,
        enumerable: true,
        configurable: false,
    });
}

// Patch permissions API to avoid 'denied' for notifications (headless default)
if (navigator.permissions) {
    const origQuery = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = (parameters) => {
        if (parameters.name === 'notifications') {
            return Promise.resolve({ state: 'prompt', onchange: null });
        }
        return origQuery(parameters);
    };
}

// Canvas fingerprint noise (tiny, imperceptible to human eye)
const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
HTMLCanvasElement.prototype.toDataURL = function (type) {
    const ctx = this.getContext('2d');
    if (ctx) {
        const imageData = ctx.getImageData(0, 0, this.width || 1, this.height || 1);
        imageData.data[0] = imageData.data[0] ^ 1; // flip one LSB
        ctx.putImageData(imageData, 0, 0);
    }
    return origToDataURL.apply(this, arguments);
};

// WebGL vendor/renderer spoof
if (typeof WebGLRenderingContext !== 'undefined') {
    const origGetParam = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function (parameter) {
        if (parameter === 37445) return 'Intel Inc.';
        if (parameter === 37446) return 'Intel Iris OpenGL Engine';
        return origGetParam.call(this, parameter);
    };
}
