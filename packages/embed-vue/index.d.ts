import type { DefineComponent } from 'vue';

export type OpenSignEventDetail = {
  documentId?: string;
  partyId?: string;
  /** On `signed`: whether every party has now signed. */
  completed?: boolean;
  /** On `error`. */
  message?: string;
};

export const OpenSignSigning: DefineComponent<
  { url: string },
  object,
  object,
  object,
  object,
  object,
  object,
  {
    loaded: (detail: OpenSignEventDetail) => void;
    signed: (detail: OpenSignEventDetail) => void;
    declined: (detail: OpenSignEventDetail) => void;
    error: (detail: OpenSignEventDetail) => void;
  }
>;
