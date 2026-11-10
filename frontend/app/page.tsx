"use client";

import { useState } from "react";
import { connectWallet } from "../lib/stellar";
import {
  createStream,
  getStream,
  withdrawStream,
  cancelStream,
  type StreamView,
} from "../lib/streams";

export default function Home() {
  const [wallet, setWallet] = useState<string>("");
  const [status, setStatus] = useState<string>("");
  const [stream, setStream] = useState<StreamView | null>(null);

  const [recipient, setRecipient] = useState("");
  const [token, setToken] = useState("");
  const [amount, setAmount] = useState("10000000");
  const [days, setDays] = useState("30");
  const [streamId, setStreamId] = useState("0");

  async function onConnect() {
    try {
      const addr = await connectWallet();
      setWallet(addr);
      setStatus(`Connected: ${addr}`);
    } catch (e) {
      setStatus(`Connect failed: ${(e as Error).message}`);
    }
  }

  async function onCreate() {
    try {
      setStatus("Building stream transaction...");
      const now = Math.floor(Date.now() / 1000);
      const start = now + 60;
      const end = start + Number(days) * 86400;
      const hash = await createStream({
        sender: wallet,
        recipient,
        token,
        amount,
        startUnix: start,
        cliffUnix: start,
        endUnix: end,
        cancellable: true,
      });
      setStatus(`Stream created: ${hash}`);
    } catch (e) {
      setStatus(`Create failed: ${(e as Error).message}`);
    }
  }

  async function onLoad() {
    try {
      const s = await getStream(Number(streamId));
      setStream(s);
      setStatus(`Loaded stream #${s.id}`);
    } catch (e) {
      setStatus(`Load failed: ${(e as Error).message}`);
    }
  }

  async function onWithdraw() {
    try {
      const hash = await withdrawStream({ caller: wallet, streamId: Number(streamId) });
      setStatus(`Withdraw submitted: ${hash}`);
    } catch (e) {
      setStatus(`Withdraw failed: ${(e as Error).message}`);
    }
  }

  async function onCancel() {
    try {
      const hash = await cancelStream({ caller: wallet, streamId: Number(streamId) });
      setStatus(`Cancel submitted: ${hash}`);
    } catch (e) {
      setStatus(`Cancel failed: ${(e as Error).message}`);
    }
  }

  const box: React.CSSProperties = {
    maxWidth: 640,
    margin: "40px auto",
    padding: 24,
    border: "1px solid #ddd",
    borderRadius: 12,
  };
  const input: React.CSSProperties = {
    width: "100%",
    padding: 8,
    margin: "4px 0 12px",
    boxSizing: "border-box",
  };
  const btn: React.CSSProperties = {
    padding: "8px 16px",
    marginRight: 8,
    marginTop: 8,
    cursor: "pointer",
  };

  return (
    <main style={box}>
      <h1>Stellar Stream Vault</h1>
      <p>Programmable payment streams on Soroban. Testnet ready.</p>
      <button style={btn} onClick={onConnect}>
        {wallet ? "Reconnect" : "Connect Freighter"}
      </button>
      {wallet && <p>Wallet: {wallet}</p>}

      <h2>Create stream</h2>
      <label>Recipient (G...)</label>
      <input style={input} value={recipient} onChange={(e) => setRecipient(e.target.value)} />
      <label>Token SAC address (C...)</label>
      <input style={input} value={token} onChange={(e) => setToken(e.target.value)} />
      <label>Amount (stroops, 7 decimals)</label>
      <input style={input} value={amount} onChange={(e) => setAmount(e.target.value)} />
      <label>Duration (days)</label>
      <input style={input} value={days} onChange={(e) => setDays(e.target.value)} />
      <button style={btn} disabled={!wallet} onClick={onCreate}>
        Create stream
      </button>

      <h2>Manage stream</h2>
      <label>Stream ID</label>
      <input style={input} value={streamId} onChange={(e) => setStreamId(e.target.value)} />
      <button style={btn} onClick={onLoad}>Load</button>
      <button style={btn} disabled={!wallet} onClick={onWithdraw}>Withdraw</button>
      <button style={btn} disabled={!wallet} onClick={onCancel}>Cancel</button>

      {stream && (
        <pre style={{ background: "#f6f6f6", padding: 12, overflowX: "auto" }}>
          {JSON.stringify(stream, null, 2)}
        </pre>
      )}
      {status && <p>{status}</p>}
    </main>
  );
}
