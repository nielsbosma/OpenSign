# @opensign/embed-vue

Shows an OpenSign signing page inside a Vue app. Turn on embedded signing and list your site under
**Admin settings → Security** first. See [Embedded signing](../../docs/embedding.md).

```vue
<script setup>
import { OpenSignSigning } from '@opensign/embed-vue';
</script>

<template>
  <OpenSignSigning :url="signingUrl" style="height: 800px" @signed="onSigned" @declined="onDeclined" />
</template>
```

`signingUrl` comes from `POST /api/v1/documents/{documentId}/parties/{partyId}/signing-link` on your own server.
Never create it in the browser with an API key.
