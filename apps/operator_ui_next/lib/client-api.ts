"use client";

export async function apiGet<T>(path: string): Promise<T> {
  return apiRequest<T>(path, { method: "GET" });
}

export async function apiRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }

  const response = await fetch(`/api/ui/${path}`, {
    cache: "no-store",
    ...init,
    headers,
  });

  const bodyText = await response.text();
  const payload = parseJson(bodyText);

  if (!response.ok) {
    let message = `Request failed with ${response.status}`;
    if (bodyText) {
      message = bodyText;
    }
    if (payload && typeof payload === "object") {
      const objectPayload = payload as Record<string, unknown>;
      if (typeof objectPayload.error === "string" && objectPayload.error.trim()) {
        message = objectPayload.error;
      } else if (
        typeof objectPayload.message === "string" &&
        objectPayload.message.trim()
      ) {
        message = objectPayload.message;
      }
    }
    throw new Error(normalizeApiError(message));
  }

  return payload as T;
}

function normalizeApiError(message: string): string {
  const normalized = message.trim();
  if (
    normalized.includes("is not mapped to any role") ||
    normalized.includes("principal user:")
  ) {
    return "Workspace identity is still syncing. Refresh in a few seconds and try again.";
  }
  return normalized;
}

export function parseJson(input: string): unknown {
  if (!input) return null;
  try {
    return JSON.parse(input);
  } catch {
    return { raw: input };
  }
}

export function formatMs(value: number | null | undefined): string {
  if (value == null) return "-";
  const date = new Date(Number(value));
  if (Number.isNaN(date.getTime())) return String(value);
  return date.toLocaleString();
}

export function shortId(value: string, head = 8, tail = 8): string {
  if (!value) return "-";
  if (value.length <= head + tail + 3) return value;
  return `${value.slice(0, head)}...${value.slice(-tail)}`;
}
