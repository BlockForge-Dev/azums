import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Azums | Durable Execution Platform",
  description:
    "Landing, customer app, and operator console for durable request execution, receipts, callbacks, and replay.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
