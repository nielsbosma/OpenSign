import type { HTMLAttributes, ReactElement } from 'react';

export type OpenSignEventDetail = {
  documentId?: string;
  partyId?: string;
  /** On `signed`: whether every party has now signed. */
  completed?: boolean;
  /** On `error`. */
  message?: string;
};

export type OpenSignSigningProps = Omit<HTMLAttributes<HTMLElement>, 'onError'> & {
  /** A party's signing link from the OpenSign API. */
  url: string;
  onLoaded?: (detail: OpenSignEventDetail) => void;
  onSigned?: (detail: OpenSignEventDetail) => void;
  onDeclined?: (detail: OpenSignEventDetail) => void;
  onError?: (detail: OpenSignEventDetail) => void;
};

export function OpenSignSigning(props: OpenSignSigningProps): ReactElement | null;
