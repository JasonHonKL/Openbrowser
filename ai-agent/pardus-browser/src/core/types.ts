// Core types for browser interaction

export interface SemanticElement {
  id?: string;
  role: string;
  name?: string;
  text?: string;
  href?: string;
  inputType?: string;
  placeholder?: string;
  children?: SemanticElement[];
}

export interface SemanticTree {
  url: string;
  title?: string;
  markdown: string;
  stats: {
    landmarks: number;
    links: number;
    headings: number;
    actions: number;
    forms: number;
    totalNodes: number;
  };
}

export interface BrowserNavigateResult {
  success: boolean;
  title?: string;
  url: string;
  markdown: string;
  stats: SemanticTree['stats'];
  error?: string;
}

export interface BrowserClickResult {
  success: boolean;
  navigated: boolean;
  url?: string;
  markdown?: string;
  stats?: SemanticTree['stats'];
  error?: string;
}

export interface BrowserFillResult {
  success: boolean;
  error?: string;
}

export interface BrowserSubmitResult {
  success: boolean;
  navigated: boolean;
  url?: string;
  markdown?: string;
  stats?: SemanticTree['stats'];
  error?: string;
}

export interface BrowserScrollResult {
  success: boolean;
  error?: string;
}

// Cookie types
export interface Cookie {
  name: string;
  value: string;
  domain?: string;
  path?: string;
  expires?: number;
  httpOnly?: boolean;
  secure?: boolean;
  sameSite?: 'Strict' | 'Lax' | 'None';
}

export interface BrowserGetCookiesResult {
  success: boolean;
  cookies: Cookie[];
  error?: string;
}

export interface BrowserSetCookieResult {
  success: boolean;
  error?: string;
}

export interface BrowserDeleteCookieResult {
  success: boolean;
  error?: string;
}

// Storage types
export interface StorageItem {
  key: string;
  value: string;
}

export interface BrowserGetStorageResult {
  success: boolean;
  items: StorageItem[];
  error?: string;
}

export interface BrowserSetStorageResult {
  success: boolean;
  error?: string;
}

export interface BrowserDeleteStorageResult {
  success: boolean;
  error?: string;
}

export interface BrowserClearStorageResult {
  success: boolean;
  error?: string;
}

export interface BrowserInstanceInfo {
  id: string;
  url?: string;
  connected: boolean;
  port: number;
}

export interface ToolResult {
  success: boolean;
  content: string;
  error?: string;
}

export interface SuggestedAction {
  action_type: string;
  element_id?: number;
  selector?: string;
  label?: string;
  reason: string;
  confidence: number;
}

export interface ActionPlanResult {
  url: string;
  suggestions: SuggestedAction[];
  page_type: string;
  has_forms: boolean;
  has_pagination: boolean;
  interactive_count: number;
}

export interface BrowserGetActionPlanResult {
  success: boolean;
  actionPlan?: ActionPlanResult;
  error?: string;
}

export interface FilledField {
  field_name: string;
  value: string;
  matched_by: string;
}

export interface UnmatchedField {
  field_name?: string;
  field_type: string;
  label?: string;
  placeholder?: string;
  required: boolean;
}

export interface BrowserAutoFillResult {
  success: boolean;
  filledFields?: FilledField[];
  unmatchedFields?: UnmatchedField[];
  error?: string;
}

export interface BrowserWaitResult {
  success: boolean;
  satisfied: boolean;
  condition: string;
  reason?: string;
  error?: string;
}
