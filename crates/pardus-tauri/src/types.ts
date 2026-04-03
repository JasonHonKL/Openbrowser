export interface InstanceInfo {
  id: string;
  port: number;
  ws_url: string;
  running: boolean;
}

export interface ChallengeInfo {
  url: string;
  status: number;
  kinds: ChallengeKind[];
  risk_score: number;
}

export type ChallengeKind =
  | "Recaptcha"
  | "Hcaptcha"
  | "Turnstile"
  | "GenericCaptcha"
  | "JsChallenge"
  | "BotProtection";

export interface ChallengeSolvedPayload {
  challenge_url: string;
  cookies: string;
  headers: Record<string, string>;
}

export interface LogEntry {
  level: "info" | "warn" | "error";
  message: string;
  timestamp: string;
}
