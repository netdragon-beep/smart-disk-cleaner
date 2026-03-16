export interface FileRecord {
  path: string;
  size: number;
  extension: string | null;
  modifiedAt: string | null;
  isEmpty: boolean;
}

export interface PathIssue {
  path: string;
  message: string;
}

export interface TypeStat {
  extension: string;
  fileCount: number;
  totalSize: number;
}

export interface AnalysisResult {
  totalFiles: number;
  totalSize: number;
  emptyFiles: FileRecord[];
  emptyDirs: string[];
  largeFiles: FileRecord[];
  typeBreakdown: TypeStat[];
}

export interface DuplicateGroup {
  hash: string;
  totalSize: number;
  files: FileRecord[];
  suggestedKeep: string | null;
}

export interface DedupResult {
  groups: DuplicateGroup[];
  failures: PathIssue[];
}

export type SuggestedAction = "delete" | "keep" | "move" | "review";
export type RiskLevel = "low" | "medium" | "high";

export interface FileSuggestion {
  path: string;
  action: SuggestedAction;
  risk: RiskLevel;
  reason: string;
}

export interface AdvisorOutput {
  source: string;
  summary: string;
  suggestions: FileSuggestion[];
}

export interface ScanReport {
  generatedAt: string;
  root: string;
  analysis: AnalysisResult;
  dedup: DedupResult;
  advisor: AdvisorOutput;
  failures: PathIssue[];
}

export type ExecutionMode = "recycle" | "move";

export type DiagnosticSeverity = "info" | "warning" | "critical";
export type DiagnosticCode =
  | "ok"
  | "not_found"
  | "permission_denied"
  | "in_use_by_another_process"
  | "locked_region"
  | "read_only"
  | "already_exists"
  | "invalid_input"
  | "directory_not_empty"
  | "unsupported"
  | "unknown";

export interface PathDiagnosis {
  path: string;
  operation: string;
  code: DiagnosticCode;
  severity: DiagnosticSeverity;
  summary: string;
  details: string[];
  suggestions: string[];
  possibleRelatedApps: string[];
  errorKind: string | null;
  rawOsError: number | null;
}

export interface OperationLogEntry {
  at: string;
  path: string;
  mode: ExecutionMode;
  dryRun: boolean;
  success: boolean;
  detail: string;
  diagnosis: PathDiagnosis | null;
}

export interface AppConfig {
  largeFileThresholdMb: number;
  maxAiItems: number;
  apiKey: string | null;
  aiBaseUrl: string;
  aiModel: string;
  excludePatterns: string[];
  theme: string;
}

// Progress event types
export interface ScanProgressEvent {
  kind: "Scan";
  phase: string;
  filesFound: number;
  dirsVisited: number;
  bytesFound: number;
  currentPath: string | null;
}

export interface DedupProgressEvent {
  kind: "Dedup";
  phase: string;
  filesHashed: number;
  filesTotal: number;
  currentPath: string | null;
}

export interface AnalyzeProgressEvent {
  kind: "Analyze";
  phase: string;
  detail: string;
}

export type ProgressEvent =
  | ScanProgressEvent
  | DedupProgressEvent
  | AnalyzeProgressEvent;
