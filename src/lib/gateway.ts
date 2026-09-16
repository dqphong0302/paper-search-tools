import { invoke, isTauri } from '@tauri-apps/api/core';

/**
 * Single source of truth for talking to the embedded localhost gateway.
 * The token is kept in memory (desktop) or sessionStorage (browser dev only)
 * and is re-attached to every request once the user protects the gateway.
 */
let gatewayPort = 8795;
let gatewayToken: string | null = null;

export const DEFAULT_GATEWAY_PORT = 8795;

export function getGatewayPort(): number {
  return gatewayPort;
}

export function setGatewayPort(port: number): void {
  if (Number.isFinite(port) && port > 0) gatewayPort = port;
}

export function setGatewayToken(token: string | null): void {
  gatewayToken = token && token.trim() ? token.trim() : null;
  try {
    if (gatewayToken) sessionStorage.setItem('sg_gateway_token', gatewayToken);
    else sessionStorage.removeItem('sg_gateway_token');
  } catch {
    /* sessionStorage unavailable — token stays in memory only */
  }
}

export function getGatewayToken(): string | null {
  return gatewayToken;
}

/**
 * Resolve the real gateway port and token. Desktop reads both over trusted Tauri
 * IPC (never the network); browser dev falls back to sessionStorage.
 */
export async function initGateway(): Promise<number> {
  let port = DEFAULT_GATEWAY_PORT;
  if (isTauri()) {
    try {
      port = await invoke<number>('get_gateway_port');
      const token = await invoke<string>('get_gateway_token');
      gatewayToken = token && token.trim() ? token.trim() : null;
    } catch {
      /* gateway not ready yet; keep defaults */
    }
  } else {
    try {
      gatewayToken = sessionStorage.getItem('sg_gateway_token');
    } catch {
      gatewayToken = null;
    }
  }
  setGatewayPort(port);
  return port;
}

export function gatewayUrl(path: string): string {
  return `http://localhost:${gatewayPort}${path}`;
}

/** fetch() against the gateway with client identity and authorization attached. */
export async function gatewayFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set('x-sg-client', 'ui');
  if (gatewayToken) headers.set('Authorization', `Bearer ${gatewayToken}`);
  return fetch(gatewayUrl(path), { ...init, headers });
}
