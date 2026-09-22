/** Decodes URL-escaped file names for display while preserving malformed literal percent text. */
export function displayFileName(path: string) {
  const fileName = path.split(/[\\/]/).pop() ?? path;
  try {
    return decodeURIComponent(fileName);
  } catch {
    return fileName;
  }
}

