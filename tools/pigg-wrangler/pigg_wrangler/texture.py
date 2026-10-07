"""
City of Heroes .texture format converter.

Handles:
  - .texture -> DDS (strip proprietary header)
  - .texture -> PNG (full decode, lossless)
  - DDS decoding: DXT1, DXT5, uncompressed RGBA/BGRA

.texture file format:
  0x00  uint32  Image data offset (points to DDS or JPEG payload)
  0x04  uint32  Image data size
  0x08  uint32  Width
  0x0C  uint32  Height
  0x10  uint32  Flags
  0x14  8 bytes Reserved
  0x1C  1 byte  Null
  0x1D  3 bytes "TX2" marker
  0x20  var     Null-terminated original filename (.dds or .jpg)
  var   var     Optional extra metadata
  [offset]      Raw image data (DDS or JPEG)

No external dependencies -- pure Python + stdlib zlib.
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass


# ---- .texture header parsing ----

@dataclass
class TextureInfo:
    """Parsed .texture file header."""
    data_offset: int        # Byte offset to DDS/JPEG data
    data_size: int          # Size of the image payload
    width: int
    height: int
    flags: int
    original_name: str      # Embedded filename (e.g. "texture_library/.../foo.dds")
    image_format: str       # "dds" or "jpeg"


def parse_texture(data: bytes) -> TextureInfo:
    """Parse the .texture header and identify the image payload."""
    if len(data) < 32:
        raise ValueError("File too small to be a .texture")

    data_offset = struct.unpack_from("<I", data, 0)[0]
    data_size = struct.unpack_from("<I", data, 4)[0]
    width = struct.unpack_from("<I", data, 8)[0]
    height = struct.unpack_from("<I", data, 12)[0]
    flags = struct.unpack_from("<I", data, 16)[0]

    # Find embedded filename after TX2 marker
    tx2_pos = data.find(b"TX2", 0, data_offset)
    original_name = ""
    if tx2_pos >= 0:
        name_start = tx2_pos + 3
        name_end = data.index(b"\x00", name_start)
        original_name = data[name_start:name_end].decode("ascii", errors="replace")

    # Detect format from magic bytes at data offset
    if data_offset + 4 > len(data):
        raise ValueError(f"Data offset {data_offset} beyond file size {len(data)}")

    magic = data[data_offset:data_offset + 4]
    if magic == b"DDS ":
        image_format = "dds"
    elif magic[:2] == b"\xff\xd8":
        image_format = "jpeg"
    else:
        raise ValueError(f"Unknown image magic: {magic.hex()}")

    return TextureInfo(
        data_offset=data_offset,
        data_size=data_size,
        width=width,
        height=height,
        flags=flags,
        original_name=original_name,
        image_format=image_format,
    )


def texture_to_dds(data: bytes) -> bytes:
    """Extract the raw DDS payload from a .texture file."""
    info = parse_texture(data)
    if info.image_format != "dds":
        raise ValueError(f"Not a DDS texture (format: {info.image_format})")
    return data[info.data_offset:]


def texture_to_jpeg(data: bytes) -> bytes:
    """Extract the raw JPEG payload from a .texture file."""
    info = parse_texture(data)
    if info.image_format != "jpeg":
        raise ValueError(f"Not a JPEG texture (format: {info.image_format})")
    return data[info.data_offset:]


# ---- DDS format parsing ----

@dataclass
class DDSInfo:
    """Parsed DDS header information."""
    width: int
    height: int
    mipmap_count: int
    pixel_format: str       # "DXT1", "DXT5", "RGBA", "BGRA", "RGB", "BGR", "L8", etc.
    fourcc: str             # Raw FourCC string or ""
    bit_count: int          # Bits per pixel (for uncompressed)
    r_mask: int
    g_mask: int
    b_mask: int
    a_mask: int
    has_alpha: bool


# DDS header constants
DDS_PIXELFORMAT_FLAGS_FOURCC = 0x4
DDS_PIXELFORMAT_FLAGS_RGB = 0x40
DDS_PIXELFORMAT_FLAGS_LUMINANCE = 0x20000
DDS_PIXELFORMAT_FLAGS_ALPHA = 0x2
DDS_PIXELFORMAT_FLAGS_ALPHAPIXELS = 0x1


def parse_dds_header(dds_data: bytes) -> DDSInfo:
    """Parse a DDS file header to determine format and dimensions."""
    if dds_data[:4] != b"DDS ":
        raise ValueError("Not a DDS file")

    # DDS_HEADER starts at offset 4, size 124 bytes
    # offset 4:  header_size (always 124)
    # offset 8:  flags
    # offset 12: height
    # offset 16: width
    # offset 20: pitchOrLinearSize
    # offset 24: depth
    # offset 28: mipmapCount
    # offset 32-76: reserved (44 bytes)
    # offset 76: DDS_PIXELFORMAT (32 bytes)
    #   76: size (always 32)
    #   80: flags
    #   84: fourCC (4 bytes)
    #   88: rgbBitCount
    #   92: rBitMask
    #   96: gBitMask
    #   100: bBitMask
    #   104: aBitMask

    height = struct.unpack_from("<I", dds_data, 12)[0]
    width = struct.unpack_from("<I", dds_data, 16)[0]
    mipmap_count = struct.unpack_from("<I", dds_data, 28)[0]

    pf_flags = struct.unpack_from("<I", dds_data, 80)[0]
    fourcc_bytes = dds_data[84:88]
    bit_count = struct.unpack_from("<I", dds_data, 88)[0]
    r_mask = struct.unpack_from("<I", dds_data, 92)[0]
    g_mask = struct.unpack_from("<I", dds_data, 96)[0]
    b_mask = struct.unpack_from("<I", dds_data, 100)[0]
    a_mask = struct.unpack_from("<I", dds_data, 104)[0]

    has_alpha = bool(pf_flags & (DDS_PIXELFORMAT_FLAGS_ALPHA | DDS_PIXELFORMAT_FLAGS_ALPHAPIXELS))

    if pf_flags & DDS_PIXELFORMAT_FLAGS_FOURCC:
        try:
            fourcc = fourcc_bytes.decode("ascii")
        except UnicodeDecodeError:
            fourcc = fourcc_bytes.hex()
        pixel_format = fourcc
    elif pf_flags & DDS_PIXELFORMAT_FLAGS_RGB:
        fourcc = ""
        if bit_count == 32:
            if r_mask == 0x00FF0000 and b_mask == 0x000000FF:
                pixel_format = "BGRA" if a_mask else "BGRX"
            elif r_mask == 0x000000FF and b_mask == 0x00FF0000:
                pixel_format = "RGBA" if a_mask else "RGBX"
            else:
                pixel_format = f"RGB32_{r_mask:08X}_{g_mask:08X}_{b_mask:08X}"
        elif bit_count == 24:
            pixel_format = "BGR" if r_mask == 0x00FF0000 else "RGB"
        elif bit_count == 16:
            pixel_format = "RGB565"
        else:
            pixel_format = f"RGB{bit_count}"
        has_alpha = has_alpha or (a_mask != 0)
    elif pf_flags & DDS_PIXELFORMAT_FLAGS_LUMINANCE:
        fourcc = ""
        pixel_format = f"L{bit_count}"
    else:
        fourcc = ""
        pixel_format = f"unknown_{pf_flags:08X}"

    return DDSInfo(
        width=width, height=height,
        mipmap_count=mipmap_count,
        pixel_format=pixel_format,
        fourcc=fourcc,
        bit_count=bit_count,
        r_mask=r_mask, g_mask=g_mask, b_mask=b_mask, a_mask=a_mask,
        has_alpha=has_alpha,
    )


# ---- DXT block decoders ----

def _rgb565_to_rgba(c: int) -> tuple[int, int, int, int]:
    """Convert a 16-bit RGB565 color to (R, G, B, A) tuple."""
    r = ((c >> 11) & 0x1F) * 255 // 31
    g = ((c >> 5) & 0x3F) * 255 // 63
    b = (c & 0x1F) * 255 // 31
    return (r, g, b, 255)


def _decode_dxt1_block(block: bytes) -> list[tuple[int, int, int, int]]:
    """Decode an 8-byte DXT1 block into 16 RGBA pixels (4x4, row-major)."""
    c0 = struct.unpack_from("<H", block, 0)[0]
    c1 = struct.unpack_from("<H", block, 2)[0]
    lookup = struct.unpack_from("<I", block, 4)[0]

    r0, g0, b0, _ = _rgb565_to_rgba(c0)
    r1, g1, b1, _ = _rgb565_to_rgba(c1)

    # Build color palette
    colors = [(r0, g0, b0, 255), (r1, g1, b1, 255)]
    if c0 > c1:
        colors.append(((2 * r0 + r1 + 1) // 3, (2 * g0 + g1 + 1) // 3, (2 * b0 + b1 + 1) // 3, 255))
        colors.append(((r0 + 2 * r1 + 1) // 3, (g0 + 2 * g1 + 1) // 3, (b0 + 2 * b1 + 1) // 3, 255))
    else:
        colors.append(((r0 + r1 + 1) // 2, (g0 + g1 + 1) // 2, (b0 + b1 + 1) // 2, 255))
        colors.append((0, 0, 0, 0))  # transparent black

    pixels = []
    for i in range(16):
        idx = (lookup >> (2 * i)) & 0x3
        pixels.append(colors[idx])
    return pixels


def _decode_dxt5_block(block: bytes) -> list[tuple[int, int, int, int]]:
    """Decode a 16-byte DXT5 block into 16 RGBA pixels (4x4, row-major)."""
    # First 8 bytes: alpha block
    a0 = block[0]
    a1 = block[1]

    # 48-bit alpha lookup table (6 bytes, 3 bits per pixel)
    alpha_bits = int.from_bytes(block[2:8], "little")

    # Build alpha palette
    alphas = [a0, a1]
    if a0 > a1:
        for i in range(1, 7):
            alphas.append(((7 - i) * a0 + i * a1 + 3) // 7)
    else:
        for i in range(1, 5):
            alphas.append(((5 - i) * a0 + i * a1 + 2) // 5)
        alphas.append(0)
        alphas.append(255)

    alpha_values = []
    for i in range(16):
        idx = (alpha_bits >> (3 * i)) & 0x7
        alpha_values.append(alphas[idx])

    # Last 8 bytes: DXT1 color block
    color_pixels = _decode_dxt1_block(block[8:16])

    # Combine color + alpha
    pixels = []
    for i in range(16):
        r, g, b, _ = color_pixels[i]
        pixels.append((r, g, b, alpha_values[i]))
    return pixels


def decode_dds_to_rgba(dds_data: bytes) -> tuple[bytes, int, int, DDSInfo]:
    """Decode a DDS file to raw RGBA pixel data.

    Returns (rgba_bytes, width, height, dds_info).
    Only decodes the first mipmap level.
    """
    info = parse_dds_header(dds_data)
    w, h = info.width, info.height

    # Pixel data starts after the 128-byte header
    # (4 magic + 124 header, but DX10 extended header adds another 20 bytes)
    pixel_offset = 128

    # Check for DX10 extended header
    if info.fourcc == "DX10":
        pixel_offset = 148

    pixel_data = dds_data[pixel_offset:]

    if info.pixel_format in ("DXT1", "NVT3"):
        # DXT1: 8 bytes per 4x4 block
        rgba = _decode_dxt_image(pixel_data, w, h, _decode_dxt1_block, 8)

    elif info.pixel_format == "DXT5":
        # DXT5: 16 bytes per 4x4 block
        rgba = _decode_dxt_image(pixel_data, w, h, _decode_dxt5_block, 16)

    elif info.pixel_format in ("DXT3",):
        # DXT3: 16 bytes per block (explicit alpha + DXT1 color)
        rgba = _decode_dxt3_image(pixel_data, w, h)

    elif info.pixel_format in ("BGRA", "BGRX"):
        # 32-bit BGRA -> RGBA
        rgba = bytearray(w * h * 4)
        for i in range(w * h):
            off = i * 4
            if off + 4 > len(pixel_data):
                break
            b, g, r, a = pixel_data[off], pixel_data[off + 1], pixel_data[off + 2], pixel_data[off + 3]
            rgba[i * 4] = r
            rgba[i * 4 + 1] = g
            rgba[i * 4 + 2] = b
            rgba[i * 4 + 3] = a if info.pixel_format == "BGRA" else 255

    elif info.pixel_format in ("RGBA", "RGBX"):
        # Already RGBA (or RGBX without alpha)
        byte_count = w * h * 4
        rgba = bytearray(pixel_data[:byte_count])
        if info.pixel_format == "RGBX":
            for i in range(3, len(rgba), 4):
                rgba[i] = 255

    elif info.pixel_format == "BGR":
        # 24-bit BGR -> RGBA
        rgba = bytearray(w * h * 4)
        for i in range(w * h):
            off = i * 3
            if off + 3 > len(pixel_data):
                break
            b, g, r = pixel_data[off], pixel_data[off + 1], pixel_data[off + 2]
            rgba[i * 4] = r
            rgba[i * 4 + 1] = g
            rgba[i * 4 + 2] = b
            rgba[i * 4 + 3] = 255

    elif info.pixel_format == "RGB":
        rgba = bytearray(w * h * 4)
        for i in range(w * h):
            off = i * 3
            if off + 3 > len(pixel_data):
                break
            r, g, b = pixel_data[off], pixel_data[off + 1], pixel_data[off + 2]
            rgba[i * 4] = r
            rgba[i * 4 + 1] = g
            rgba[i * 4 + 2] = b
            rgba[i * 4 + 3] = 255

    elif info.pixel_format.startswith("L"):
        # Luminance (grayscale)
        bits = info.bit_count
        if bits == 8:
            rgba = bytearray(w * h * 4)
            for i in range(min(w * h, len(pixel_data))):
                v = pixel_data[i]
                rgba[i * 4] = v
                rgba[i * 4 + 1] = v
                rgba[i * 4 + 2] = v
                rgba[i * 4 + 3] = 255
        else:
            raise ValueError(f"Unsupported luminance bit depth: {bits}")

    else:
        raise ValueError(f"Unsupported DDS pixel format: {info.pixel_format}")

    return bytes(rgba), w, h, info


def _decode_dxt_image(pixel_data: bytes, w: int, h: int,
                      block_decoder, block_size: int) -> bytes:
    """Decode a DXT-compressed image to RGBA bytes."""
    # DXT operates on 4x4 blocks; dimensions are rounded up to multiples of 4
    bw = (w + 3) // 4  # blocks wide
    bh = (h + 3) // 4  # blocks tall
    rgba = bytearray(w * h * 4)

    for by in range(bh):
        for bx in range(bw):
            block_idx = by * bw + bx
            offset = block_idx * block_size
            if offset + block_size > len(pixel_data):
                break

            block = pixel_data[offset:offset + block_size]
            pixels = block_decoder(block)

            # Place 4x4 block into the output image
            for py in range(4):
                for px in range(4):
                    ix = bx * 4 + px
                    iy = by * 4 + py
                    if ix < w and iy < h:
                        pi = py * 4 + px
                        out_idx = (iy * w + ix) * 4
                        r, g, b, a = pixels[pi]
                        rgba[out_idx] = r
                        rgba[out_idx + 1] = g
                        rgba[out_idx + 2] = b
                        rgba[out_idx + 3] = a

    return bytes(rgba)


def _decode_dxt3_image(pixel_data: bytes, w: int, h: int) -> bytes:
    """Decode DXT3 compressed image (explicit alpha + DXT1 color)."""
    bw = (w + 3) // 4
    bh = (h + 3) // 4
    rgba = bytearray(w * h * 4)

    for by in range(bh):
        for bx in range(bw):
            block_idx = by * bw + bx
            offset = block_idx * 16
            if offset + 16 > len(pixel_data):
                break

            block = pixel_data[offset:offset + 16]

            # First 8 bytes: explicit alpha (4 bits per pixel)
            alpha_data = struct.unpack_from("<Q", block, 0)[0]
            alpha_values = []
            for i in range(16):
                a4 = (alpha_data >> (4 * i)) & 0xF
                alpha_values.append(a4 * 17)  # scale 0-15 to 0-255

            # Last 8 bytes: DXT1 color block
            color_pixels = _decode_dxt1_block(block[8:16])

            for py in range(4):
                for px in range(4):
                    ix = bx * 4 + px
                    iy = by * 4 + py
                    if ix < w and iy < h:
                        pi = py * 4 + px
                        out_idx = (iy * w + ix) * 4
                        r, g, b, _ = color_pixels[pi]
                        rgba[out_idx] = r
                        rgba[out_idx + 1] = g
                        rgba[out_idx + 2] = b
                        rgba[out_idx + 3] = alpha_values[pi]

    return bytes(rgba)


# ---- PNG encoder (pure Python, no dependencies) ----

def rgba_to_png(pixels: bytes, width: int, height: int) -> bytes:
    """Encode raw RGBA pixels to a PNG file.

    Uses zlib compression from stdlib. No external dependencies.
    """
    # Build raw scanlines with filter byte (0 = None filter for simplicity)
    raw_data = bytearray()
    stride = width * 4
    for y in range(height):
        raw_data.append(0)  # filter type: None
        row_start = y * stride
        raw_data.extend(pixels[row_start:row_start + stride])

    compressed = zlib.compress(bytes(raw_data), 9)

    # Build PNG file
    png = bytearray()

    # PNG signature
    png.extend(b"\x89PNG\r\n\x1a\n")

    # IHDR chunk
    ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    # bit_depth=8, color_type=6 (RGBA), compression=0, filter=0, interlace=0
    _write_png_chunk(png, b"IHDR", ihdr_data)

    # IDAT chunk
    _write_png_chunk(png, b"IDAT", compressed)

    # IEND chunk
    _write_png_chunk(png, b"IEND", b"")

    return bytes(png)


def _write_png_chunk(png: bytearray, chunk_type: bytes, data: bytes) -> None:
    """Write a PNG chunk: length + type + data + CRC32."""
    png.extend(struct.pack(">I", len(data)))
    png.extend(chunk_type)
    png.extend(data)
    crc = zlib.crc32(chunk_type + data) & 0xFFFFFFFF
    png.extend(struct.pack(">I", crc))


# ---- RGBA trimming ----

def trim_rgba(pixels: bytes, width: int, height: int) -> tuple[bytes, int, int]:
    """Trim transparent borders from RGBA pixel data.

    Scans for the bounding box of non-transparent (alpha > 0) pixels and
    returns cropped pixel data with updated dimensions.
    Returns (trimmed_rgba, new_width, new_height).
    If the image is fully transparent, returns a 1x1 transparent pixel.
    """
    min_x, min_y = width, height
    max_x, max_y = -1, -1

    for y in range(height):
        row_offset = y * width * 4
        for x in range(width):
            alpha = pixels[row_offset + x * 4 + 3]
            if alpha > 0:
                if x < min_x: min_x = x
                if x > max_x: max_x = x
                if y < min_y: min_y = y
                if y > max_y: max_y = y

    if max_x < 0:
        # Fully transparent image
        return b"\x00\x00\x00\x00", 1, 1

    new_w = max_x - min_x + 1
    new_h = max_y - min_y + 1
    trimmed = bytearray(new_w * new_h * 4)

    for y in range(new_h):
        src_offset = ((min_y + y) * width + min_x) * 4
        dst_offset = y * new_w * 4
        trimmed[dst_offset:dst_offset + new_w * 4] = pixels[src_offset:src_offset + new_w * 4]

    return bytes(trimmed), new_w, new_h


# ---- High-level conversion functions ----

def texture_to_png(texture_data: bytes) -> bytes:
    """Convert a .texture file to PNG format.

    Handles both DDS textures (decoded) and JPEG textures (returned as-is since
    browsers can display JPEG natively).
    """
    info = parse_texture(texture_data)

    if info.image_format == "jpeg":
        # JPEG can be used directly -- no conversion needed
        return texture_data[info.data_offset:]

    # DDS path: extract, decode, encode as PNG
    dds_data = texture_data[info.data_offset:]
    rgba, w, h, dds_info = decode_dds_to_rgba(dds_data)
    return rgba_to_png(rgba, w, h)


def texture_to_png_trimmed(texture_data: bytes) -> bytes:
    """Convert a .texture file to a trimmed PNG (transparent borders removed).

    For JPEG textures, returns the JPEG as-is (no transparent borders to trim).
    """
    info = parse_texture(texture_data)

    if info.image_format == "jpeg":
        return texture_data[info.data_offset:]

    dds_data = texture_data[info.data_offset:]
    rgba, w, h, dds_info = decode_dds_to_rgba(dds_data)
    trimmed, tw, th = trim_rgba(rgba, w, h)
    return rgba_to_png(trimmed, tw, th)


def texture_to_image(texture_data: bytes) -> tuple[bytes, str]:
    """Convert a .texture file to a browser-displayable image.

    Returns (image_bytes, mime_type).
    """
    info = parse_texture(texture_data)

    if info.image_format == "jpeg":
        return texture_data[info.data_offset:], "image/jpeg"

    dds_data = texture_data[info.data_offset:]
    rgba, w, h, dds_info = decode_dds_to_rgba(dds_data)
    png_data = rgba_to_png(rgba, w, h)
    return png_data, "image/png"


def get_texture_info(texture_data: bytes) -> dict:
    """Get detailed info about a .texture file for the UI."""
    info = parse_texture(texture_data)
    result = {
        "width": info.width,
        "height": info.height,
        "image_format": info.image_format,
        "original_name": info.original_name,
        "data_offset": info.data_offset,
        "data_size": info.data_size,
        "flags": f"0x{info.flags:08X}",
    }

    if info.image_format == "dds":
        dds_data = texture_data[info.data_offset:]
        dds_info = parse_dds_header(dds_data)
        result["dds_format"] = dds_info.pixel_format
        result["dds_fourcc"] = dds_info.fourcc
        result["has_alpha"] = dds_info.has_alpha
        result["mipmap_count"] = dds_info.mipmap_count
        result["bit_count"] = dds_info.bit_count

    return result
