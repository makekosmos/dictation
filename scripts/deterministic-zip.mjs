const UTF8 = 0x0800;
const DOS_DATE_1980_01_01 = 0x0021;

const crcTable = Array.from({ length: 256 }, (_, value) => {
  let crc = value;
  for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  return crc >>> 0;
});

function crc32(data) {
  let crc = 0xffffffff;
  for (const byte of data) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 0xff];
  return (crc ^ 0xffffffff) >>> 0;
}

function safeName(name) {
  const normalized = name.replaceAll("\\", "/");
  const withoutTrailingSlash = normalized.endsWith("/") ? normalized.slice(0, -1) : normalized;
  if (
    !withoutTrailingSlash ||
    normalized.startsWith("/") ||
    normalized.includes("\0") ||
    withoutTrailingSlash
      .split("/")
      .some((part) => !part || part === "." || part === ".." || part.includes(":"))
  ) {
    throw new Error(`unsafe ZIP entry: ${name}`);
  }
  return normalized;
}

export function createDeterministicZip(inputFiles) {
  const files = inputFiles
    .map(({ name, data, directory = name.endsWith("/") }) => {
      const safe = safeName(name);
      const bytes = Buffer.from(data);
      if (directory && bytes.length !== 0) throw new Error(`directory has content: ${name}`);
      return { name: safe, data: bytes, directory };
    })
    .sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0));
  if (new Set(files.map(({ name }) => name)).size !== files.length) {
    throw new Error("duplicate ZIP entry");
  }

  const localParts = [];
  const centralParts = [];
  let offset = 0;
  for (const { name, data, directory } of files) {
    const encoded = Buffer.from(name, "utf8");
    const crc = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(UTF8, 6);
    local.writeUInt16LE(0, 8);
    local.writeUInt16LE(0, 10);
    local.writeUInt16LE(DOS_DATE_1980_01_01, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(encoded.length, 26);
    localParts.push(local, encoded, data);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(UTF8, 8);
    central.writeUInt16LE(0, 10);
    central.writeUInt16LE(0, 12);
    central.writeUInt16LE(DOS_DATE_1980_01_01, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(encoded.length, 28);
    central.writeUInt32LE(directory ? 0x10 : 0, 38);
    central.writeUInt32LE(offset, 42);
    centralParts.push(central, encoded);
    offset += local.length + encoded.length + data.length;
  }

  const central = Buffer.concat(centralParts);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(central.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...localParts, central, end]);
}

export function readCanonicalZip(archive) {
  const data = Buffer.from(archive);
  const files = [];
  let offset = 0;
  while (offset + 4 <= data.length && data.readUInt32LE(offset) === 0x04034b50) {
    if (offset + 30 > data.length) throw new Error("truncated ZIP local header");
    const flags = data.readUInt16LE(offset + 6);
    const method = data.readUInt16LE(offset + 8);
    const compressedSize = data.readUInt32LE(offset + 18);
    const size = data.readUInt32LE(offset + 22);
    const nameLength = data.readUInt16LE(offset + 26);
    const extraLength = data.readUInt16LE(offset + 28);
    if (flags !== UTF8 || method !== 0 || compressedSize !== size || extraLength !== 0) {
      throw new Error("ZIP is not canonical stored UTF-8 format");
    }
    const nameStart = offset + 30;
    const contentStart = nameStart + nameLength;
    const contentEnd = contentStart + size;
    if (contentEnd > data.length) throw new Error("truncated ZIP entry");
    const name = safeName(data.subarray(nameStart, contentStart).toString("utf8"));
    const content = data.subarray(contentStart, contentEnd);
    if (crc32(content) !== data.readUInt32LE(offset + 14)) throw new Error(`invalid CRC: ${name}`);
    files.push({ name, data: Buffer.from(content), directory: name.endsWith("/") });
    offset = contentEnd;
  }
  if (files.length === 0 || !data.equals(createDeterministicZip(files))) {
    throw new Error("ZIP bytes are not deterministic canonical output");
  }
  return files;
}
