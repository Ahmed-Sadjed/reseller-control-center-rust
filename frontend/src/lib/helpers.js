export async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function downloadTextFile(content, filename) {
  const blob = new Blob([content], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * CRITICAL: Sanitize filename for Windows compatibility.
 * Replace : and / with _ to prevent filesystem errors.
 */
export function sanitizeFilename(filename) {
  return filename.replace(/[:/\\]/g, '_');
}

/**
 * Legacy formatter — used when no display schema is available.
 */
export function formatCredentialBlock(username, password, dns, m3uUrl) {
  let block = `Username: ${username}\nPassword: ${password}\nDNS: ${dns}`;
  if (m3uUrl) {
    block += `\nM3U URL: ${m3uUrl}`;
  }
  return block;
}

/**
 * Schema-driven formatter — builds text block from display schema fields.
 * Only includes fields defined in the schema (in order).
 */
export function formatCredentialBlockFromSchema(fields, credentialData) {
  if (!fields || !credentialData) return '';
  return fields
    .map((field) => {
      const value = credentialData[field.key] ?? '';
      return `${field.label}: ${value}`;
    })
    .join('\n');
}
