import { NextRequest, NextResponse } from "next/server";

const HOP_BY_HOP = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);

const FORWARDED_REQUEST_HEADERS = new Set(["x-azums-submit-surface"]);

function backendOrigin(): string {
  return process.env.OPERATOR_UI_BACKEND_ORIGIN ?? "http://127.0.0.1:8083";
}

async function proxy(request: NextRequest, pathParts: string[]): Promise<NextResponse> {
  const joinedPath = pathParts.join("/");
  const target = new URL(`/api/ui/${joinedPath}`, backendOrigin());
  target.search = request.nextUrl.search;

  const headers = new Headers();
  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }
  const cookie = request.headers.get("cookie");
  if (cookie) {
    headers.set("cookie", cookie);
  }
  request.headers.forEach((value, key) => {
    const lower = key.toLowerCase();
    if (FORWARDED_REQUEST_HEADERS.has(lower) && value.trim()) {
      headers.set(key, value);
    }
  });

  const init: RequestInit = {
    method: request.method,
    headers,
    cache: "no-store",
  };

  if (!["GET", "HEAD"].includes(request.method.toUpperCase())) {
    init.body = await request.text();
  }

  try {
    const upstream = await fetch(target, init);
    const body = await upstream.text();

    const responseHeaders = new Headers();
    const upstreamContentType = upstream.headers.get("content-type");
    if (upstreamContentType) {
      responseHeaders.set("content-type", upstreamContentType);
    }

    upstream.headers.forEach((value, key) => {
      const lower = key.toLowerCase();
      if (HOP_BY_HOP.has(lower)) return;
      if (lower.startsWith("x-")) {
        responseHeaders.set(key, value);
        return;
      }
      if (lower === "set-cookie") {
        responseHeaders.append(key, value);
      }
    });

    return new NextResponse(body, {
      status: upstream.status,
      headers: responseHeaders,
    });
  } catch (error) {
    return NextResponse.json(
      {
        ok: false,
        error: `Failed to reach operator_ui backend at ${backendOrigin()}: ${String(error)}`,
      },
      { status: 502 }
    );
  }
}

type RouteContext = {
  params: Promise<{
    path?: string[];
  }>;
};

async function readPathParts(context: RouteContext): Promise<string[]> {
  const params = await context.params;
  return params.path ?? [];
}

export async function GET(request: NextRequest, context: RouteContext): Promise<NextResponse> {
  return proxy(request, await readPathParts(context));
}

export async function POST(request: NextRequest, context: RouteContext): Promise<NextResponse> {
  return proxy(request, await readPathParts(context));
}

export async function PUT(request: NextRequest, context: RouteContext): Promise<NextResponse> {
  return proxy(request, await readPathParts(context));
}

export async function PATCH(request: NextRequest, context: RouteContext): Promise<NextResponse> {
  return proxy(request, await readPathParts(context));
}

export async function DELETE(request: NextRequest, context: RouteContext): Promise<NextResponse> {
  return proxy(request, await readPathParts(context));
}
