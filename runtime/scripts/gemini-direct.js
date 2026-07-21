#!/usr/bin/env node
// Get Gemini quota from Cloud AI Companion API with OAuth auto-refresh.

const https = require('https');
const fs = require('fs');
const path = require('path');

const USER_HOME = process.env.USERPROFILE || 'C:/Users/Administrator';
const APPDATA = process.env.APPDATA || path.join(USER_HOME, 'AppData', 'Roaming');
const CREDS_PATH = path.join(USER_HOME, '.gemini', 'oauth_creds.json');
const GEMINI_BUNDLE_DIR = path.join(
  APPDATA,
  'npm',
  'node_modules',
  '@google',
  'gemini-cli',
  'bundle',
);
const API_URL = 'https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota';
const DEFAULT_CLIENT_ID =
  '681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com';

function fmtCountdown(resetTime) {
  const deltaMs = new Date(resetTime) - new Date();
  if (deltaMs <= 0) {
    return 'resetting...';
  }
  const hours = Math.floor(deltaMs / 3600000);
  const minutes = Math.floor((deltaMs % 3600000) / 60000);
  return hours > 0 ? `${hours}h${minutes}m` : `${minutes}m`;
}

function shortName(modelId) {
  return modelId.replace('gemini-', '').replace('-preview', '');
}

function locateGeminiOauthConfig() {
  if (process.env.GEMINI_CLIENT_ID || process.env.GEMINI_CLIENT_SECRET) {
    return {
      clientId: process.env.GEMINI_CLIENT_ID || DEFAULT_CLIENT_ID,
      clientSecret: process.env.GEMINI_CLIENT_SECRET || '',
      source: 'env',
    };
  }

  try {
    const files = fs
      .readdirSync(GEMINI_BUNDLE_DIR)
      .filter((file) => /^chunk-.*\.js$/i.test(file))
      .sort();

    for (const file of files) {
      const fullPath = path.join(GEMINI_BUNDLE_DIR, file);
      const source = fs.readFileSync(fullPath, 'utf8');
      const idMatch = source.match(/var OAUTH_CLIENT_ID = "([^"]+)";/);
      const secretMatch = source.match(/var OAUTH_CLIENT_SECRET = "([^"]+)";/);
      if (idMatch && secretMatch) {
        return {
          clientId: idMatch[1],
          clientSecret: secretMatch[1],
          source: fullPath,
        };
      }
    }
  } catch {
    // Fall through to default config.
  }

  return {
    clientId: DEFAULT_CLIENT_ID,
    clientSecret: '',
    source: 'fallback',
  };
}

function fetchQuota(accessToken) {
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({ project: 'administrator' });
    const url = new URL(API_URL);
    const request = https.request(
      {
        hostname: url.hostname,
        path: url.pathname,
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${accessToken}`,
          'Content-Length': Buffer.byteLength(body),
        },
      },
      (response) => {
        let raw = '';
        response.on('data', (chunk) => {
          raw += chunk;
        });
        response.on('end', () => {
          if (response.statusCode === 401) {
            reject(new Error('TOKEN_EXPIRED'));
            return;
          }
          if (response.statusCode !== 200) {
            reject(new Error(`API ${response.statusCode}`));
            return;
          }
          resolve(JSON.parse(raw));
        });
      },
    );
    request.on('error', reject);
    request.setTimeout(10000, () => {
      request.destroy();
      reject(new Error('timeout'));
    });
    request.write(body);
    request.end();
  });
}

function refreshToken(refreshTokenValue, oauthConfig) {
  return new Promise((resolve, reject) => {
    const params = new URLSearchParams({
      client_id: oauthConfig.clientId,
      refresh_token: refreshTokenValue,
      grant_type: 'refresh_token',
    });
    if (oauthConfig.clientSecret) {
      params.set('client_secret', oauthConfig.clientSecret);
    }

    const body = params.toString();
    const request = https.request(
      {
        hostname: 'oauth2.googleapis.com',
        path: '/token',
        method: 'POST',
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded',
          'Content-Length': Buffer.byteLength(body),
        },
      },
      (response) => {
        let raw = '';
        response.on('data', (chunk) => {
          raw += chunk;
        });
        response.on('end', () => {
          if (response.statusCode !== 200) {
            reject(new Error(`refresh ${response.statusCode}`));
            return;
          }
          const parsed = JSON.parse(raw);
          resolve(parsed.access_token);
        });
      },
    );
    request.on('error', reject);
    request.setTimeout(10000, () => {
      request.destroy();
      reject(new Error('refresh timeout'));
    });
    request.write(body);
    request.end();
  });
}

function emitSuccess(response, oauthSource) {
  const buckets = response.buckets || [];
  const lines = [];
  let lowestRemaining = 100;

  for (const bucket of buckets) {
    const pct = Math.round((bucket.remainingFraction || 1) * 100);
    const label = shortName(bucket.modelId || '?');
    const reset = bucket.resetTime ? fmtCountdown(bucket.resetTime) : '?';
    lines.push(`${label}: ${pct}% (${reset})`);
    if (pct < lowestRemaining) {
      lowestRemaining = pct;
    }
  }

  console.log(
    JSON.stringify({
      ok: true,
      remaining_pct: lowestRemaining,
      quota_lines: lines.join('\n'),
      models_count: buckets.length,
      tier: 'Google One AI Premium',
      oauth_source: oauthSource,
      ts: new Date().toISOString(),
    }),
  );
}

(async () => {
  try {
    const creds = JSON.parse(fs.readFileSync(CREDS_PATH, 'utf8'));
    const oauthConfig = locateGeminiOauthConfig();
    let token = creds.access_token;

    try {
      emitSuccess(await fetchQuota(token), oauthConfig.source);
      return;
    } catch (error) {
      if (error.message !== 'TOKEN_EXPIRED' || !creds.refresh_token) {
        throw error;
      }
    }

    token = await refreshToken(creds.refresh_token, oauthConfig);
    creds.access_token = token;
    creds.expiry_date = Date.now() + 3600000;
    fs.writeFileSync(CREDS_PATH, JSON.stringify(creds, null, 2));
    emitSuccess(await fetchQuota(token), oauthConfig.source);
  } catch (error) {
    console.log(JSON.stringify({ ok: false, error: error.message }));
    process.exit(1);
  }
})();
