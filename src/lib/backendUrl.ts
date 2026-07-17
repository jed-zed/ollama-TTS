import { API_BASE_URL } from '@/lib/constants';

let backendBaseUrl = API_BASE_URL;

const normalizeBaseUrl = (url: string): string => url.trim().replace(/\/+$/, '');

/**
 * Returns the backend URL selected during application bootstrap.
 * Browser builds keep using the local development fallback.
 */
export const getBackendBaseUrl = (): string => backendBaseUrl;

export const setBackendBaseUrl = (url: string): void => {
  const normalizedUrl = normalizeBaseUrl(url);
  if (normalizedUrl) {
    backendBaseUrl = normalizedUrl;
  }
};

const isTauriRuntime = (): boolean => {
  if (typeof window === 'undefined') return false;
  return '__TAURI_INTERNALS__' in window || '__TAURI__' in window;
};

/**
 * Resolves the configured backend URL before React mounts and starts requests.
 */
export const initializeBackendBaseUrl = async (): Promise<string> => {
  if (!isTauriRuntime()) return backendBaseUrl;

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const configuredUrl = await invoke<string>('get_backend_url');
    setBackendBaseUrl(configuredUrl);
  } catch (error) {
    console.warn('[Backend URL] Failed to load configured URL, using fallback:', error);
  }

  return backendBaseUrl;
};
