import * as StellarSdk from "@stellar/stellar-sdk";
import { getConfig, invokeContract, sc, server } from "./stellar";

export interface StreamView {
  id: number;
  sender: string;
  recipient: string;
  token: string;
  deposit: string;
  withdrawn: string;
  start: number;
  cliff: number;
  end: number;
  cancellable: boolean;
  cancelled: boolean;
  created_at: number;
}

/** Read-only: fetch a stream via simulate (no wallet signature needed). */
export async function getStream(streamId: number): Promise<StreamView> {
  const { contractId, passphrase } = getConfig();
  if (!contractId) throw new Error("Missing NEXT_PUBLIC_VAULT_CONTRACT_ID");
  const srv = server();
  // Use a dummy source for simulation-only reads.
  const dummy = StellarSdk.Keypair.random().publicKey();
  const account = new StellarSdk.Account(dummy, "0");
  const contract = new StellarSdk.Contract(contractId);
  const tx = new StellarSdk.TransactionBuilder(account, {
    fee: StellarSdk.BASE_FEE,
    networkPassphrase: passphrase,
  })
    .addOperation(contract.call("get_stream", sc.u64(streamId)))
    .setTimeout(30)
    .build();
  const sim = await srv.simulateTransaction(tx);
  if (StellarSdk.rpc.Api.isSimulationError(sim)) {
    throw new Error("Stream not found or simulation failed");
  }
  const rv = (sim as StellarSdk.rpc.Api.SimulateTransactionSuccessResponse)
    .result?.retval;
  if (!rv) throw new Error("Empty simulation result");
  const decoded = StellarSdk.scValToNative(rv) as Record<string, unknown>;
  return {
    id: streamId,
    sender: String(decoded.sender),
    recipient: String(decoded.recipient),
    token: String(decoded.token),
    deposit: String(decoded.deposit),
    withdrawn: String(decoded.withdrawn),
    start: Number(decoded.start),
    cliff: Number(decoded.cliff),
    end: Number(decoded.end),
    cancellable: Boolean(decoded.cancellable),
    cancelled: Boolean(decoded.cancelled),
    created_at: Number(decoded.created_at),
  };
}

/** Create a stream (sender signs via Freighter). Returns tx hash. */
export async function createStream(params: {
  sender: string;
  recipient: string;
  token: string;
  amount: string;
  startUnix: number;
  cliffUnix: number;
  endUnix: number;
  cancellable: boolean;
}): Promise<string> {
  const { contractId } = getConfig();
  const { hash } = await invokeContract({
    sourcePublicKey: params.sender,
    contractId,
    method: "create_stream",
    args: [
      sc.address(params.sender),
      sc.address(params.recipient),
      sc.address(params.token),
      sc.i128(params.amount),
      sc.u64(params.startUnix),
      sc.u64(params.cliffUnix),
      sc.u64(params.endUnix),
      sc.bool(params.cancellable),
    ],
  });
  return hash;
}

/** Recipient withdraws vested funds. */
export async function withdrawStream(params: {
  caller: string;
  streamId: number;
}): Promise<string> {
  const { contractId } = getConfig();
  const { hash } = await invokeContract({
    sourcePublicKey: params.caller,
    contractId,
    method: "withdraw",
    args: [sc.u64(params.streamId)],
  });
  return hash;
}

/** Sender cancels a cancellable stream. */
export async function cancelStream(params: {
  caller: string;
  streamId: number;
}): Promise<string> {
  const { contractId } = getConfig();
  const { hash } = await invokeContract({
    sourcePublicKey: params.caller,
    contractId,
    method: "cancel",
    args: [sc.u64(params.streamId)],
  });
  return hash;
}
