/** A PIN that keeps a device in show mode, kept as a salted hash: never the PIN itself. */
export interface PinLock {
  salt: string;
  hash: string;
}

/** The fewest characters a PIN has; a lock is to stop accidents, not to guard secrets. */
export const MIN_PIN = 4;

export function validPin(pin: string): boolean {
  return pin.length >= MIN_PIN;
}

const HEX = /^[0-9a-f]+$/;

/** What was stored, if it is a lock. */
export function normalizeLock(saved: unknown): PinLock | null {
  if (typeof saved !== "object" || saved === null) return null;
  const { salt, hash } = saved as Record<string, unknown>;
  return typeof salt === "string" && typeof hash === "string" && HEX.test(salt) && HEX.test(hash) && hash.length === 64 ? { salt, hash } : null;
}

function toHex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function sha256Hex(text: string): Promise<string> {
  return toHex(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text))));
}

export function randomSalt(): string {
  return toHex(crypto.getRandomValues(new Uint8Array(16)));
}

export async function makeLock(pin: string, salt: string = randomSalt()): Promise<PinLock> {
  return { salt, hash: await sha256Hex(`${salt}:${pin}`) };
}

export async function checkPin(pin: string, lock: PinLock): Promise<boolean> {
  const attempt = await sha256Hex(`${lock.salt}:${pin}`);
  // Every character is compared, whatever the first difference.
  let difference = attempt.length ^ lock.hash.length;
  for (let index = 0; index < attempt.length; index++) difference |= attempt.charCodeAt(index) ^ (lock.hash.charCodeAt(index) || 0);
  return difference === 0;
}
