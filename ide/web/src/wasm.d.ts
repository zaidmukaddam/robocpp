declare module "./wasm/iec_language_service_wasm/iec_language_service_wasm.js" {
  export default function init(): Promise<void>;
  export function analyze_document_json(uri: string, text: string, languageId?: string): string;
  export function graph_model_json(uri: string, text: string, languageId?: string): string;
  export function validate_graph_json(uri: string, text: string, languageId?: string): string;
  export function debug_document_json(
    uri: string,
    text: string,
    languageId?: string,
    cycles?: number
  ): string;
  export function generated_c_artifact_json(uri: string, text: string, languageId?: string): string;
  export function capabilities_json(): string;
}
