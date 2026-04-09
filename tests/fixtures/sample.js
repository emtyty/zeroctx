import { readFile, writeFile } from 'fs/promises';

const MAX_RETRIES = 3;
const TIMEOUT = 5000;

function parseConfig(raw) {
    const lines = raw.split('\n');
    const config = {};
    for (const line of lines) {
        const [key, value] = line.split('=');
        if (key && value) {
            config[key.trim()] = value.trim();
        }
    }
    return config;
}

class ApiClient {
    constructor(baseUrl, token) {
        this.baseUrl = baseUrl;
        this.token = token;
        this.retries = MAX_RETRIES;
    }

    async get(endpoint) {
        const url = `${this.baseUrl}/${endpoint}`;
        const resp = await fetch(url, {
            headers: { Authorization: `Bearer ${this.token}` },
            signal: AbortSignal.timeout(TIMEOUT),
        });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        return resp.json();
    }

    async post(endpoint, data) {
        const url = `${this.baseUrl}/${endpoint}`;
        const resp = await fetch(url, {
            method: 'POST',
            headers: {
                Authorization: `Bearer ${this.token}`,
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(data),
        });
        return resp.json();
    }
}

function debounce(fn, delay) {
    let timer;
    return (...args) => {
        clearTimeout(timer);
        timer = setTimeout(() => fn(...args), delay);
    };
}

export { ApiClient, parseConfig, debounce };
// line 62
// line 63
// line 64
// line 65
// line 66
// line 67
// line 68
// line 69
// line 70
// line 71
// line 72
// line 73
// line 74
// line 75
// line 76
// line 77
// line 78
// line 79
// line 80
// line 81
// line 82
// line 83
// line 84
// line 85
