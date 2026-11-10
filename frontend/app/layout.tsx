import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "Stellar Stream Vault",
  description: "Real-time programmable payment streams on Soroban.",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body style={{ fontFamily: "system-ui, sans-serif", margin: 0 }}>
        {children}
      </body>
    </html>
  );
}
