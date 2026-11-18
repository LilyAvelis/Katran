using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace QuickLook.Plugin.BullViewer
{
    internal sealed class BullDocument : IDisposable
    {
        private const uint MaxPages = 1024;
        private const long MaxTotal = 256L * 1024 * 1024;
        private const int MaxPage = 64 * 1024 * 1024;
        private readonly FileStream stream;
        private readonly List<Page> pages = new List<Page>();
        private readonly object gate = new object();
        private sealed class Page { public long Offset; public int Length; public byte[] Digest; }
        private static readonly uint[] CrcTable = CreateCrcTable();
        private BullDocument(FileStream stream) { this.stream = stream; }
        public int Count { get { return pages.Count; } }

        public static BullDocument Open(string path)
        {
            var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
            var result = new BullDocument(file);
            try { result.Index(); return result; }
            catch { result.Dispose(); throw; }
        }
        private void Index()
        {
            if (stream.Length < 16 || stream.Length > MaxTotal) throw Invalid("Invalid collection size");
            using (var reader = new BinaryReader(stream, Encoding.UTF8, true))
            {
                if (Encoding.ASCII.GetString(ReadExact(reader, 8)) != "BALOHCOL") throw Invalid("Invalid collection magic");
                if (reader.ReadUInt32() != 1) throw Invalid("Unsupported collection version");
                uint count = reader.ReadUInt32();
                if (count < 1 || count > MaxPages) throw Invalid("Invalid page count");
                for (int i = 0; i < count; i++)
                {
                    uint length = reader.ReadUInt32();
                    if (length == 0 || length > MaxPage) throw Invalid("Invalid page length");
                    byte[] digest = ReadExact(reader, 32);
                    long offset = stream.Position;
                    if ((long)length > stream.Length - offset) throw Invalid("Truncated page");
                    var page = new Page { Offset = offset, Length = (int)length, Digest = digest };
                    Validate(ReadExact(reader, page.Length), page.Digest);
                    pages.Add(page);
                }
                if (stream.Position != stream.Length) throw Invalid("Trailing collection data");
            }
        }
        public byte[] ReadPage(int index)
        {
            lock (gate)
            {
                if (index < 0 || index >= pages.Count) throw Invalid("Page out of range");
                var page = pages[index];
                stream.Position = page.Offset;
                using (var reader = new BinaryReader(stream, Encoding.UTF8, true))
                {
                    byte[] bytes = ReadExact(reader, page.Length);
                    Validate(bytes, page.Digest);
                    return bytes;
                }
            }
        }
        private static byte[] ReadExact(BinaryReader reader, int count)
        {
            byte[] bytes = reader.ReadBytes(count);
            if (bytes.Length != count) throw Invalid("Truncated collection");
            return bytes;
        }
        private static InvalidDataException Invalid(string message) { return new InvalidDataException(message); }
        private static uint Big(byte[] bytes, int offset)
        {
            return ((uint)bytes[offset] << 24) | ((uint)bytes[offset+1] << 16) | ((uint)bytes[offset+2] << 8) | bytes[offset+3];
        }
        private static uint[] CreateCrcTable()
        {
            var table = new uint[256];
            for (uint i = 0; i < 256; i++) { uint x = i; for (int j = 0; j < 8; j++) x = (x & 1) != 0 ? 0xedb88320U ^ (x >> 1) : x >> 1; table[i] = x; }
            return table;
        }
        private static uint Crc(byte[] bytes, int start, int end)
        {
            uint crc = 0xffffffff;
            for (int i = start; i < end; i++) crc = CrcTable[(crc ^ bytes[i]) & 255] ^ (crc >> 8);
            return crc ^ 0xffffffff;
        }
        private static void Validate(byte[] png, byte[] digest)
        {
            using (var sha = SHA256.Create())
            {
                byte[] actual = sha.ComputeHash(png);
                int diff = 0; for (int i = 0; i < 32; i++) diff |= actual[i] ^ digest[i];
                if (diff != 0) throw Invalid("Page checksum mismatch");
            }
            byte[] signature = { 137,80,78,71,13,10,26,10 };
            if (png.Length < 45) throw Invalid("Invalid PNG");
            for (int i = 0; i < 8; i++) if (png[i] != signature[i]) throw Invalid("Invalid PNG signature");
            int offset = 8, chunks = 0; bool imageData = false, ended = false;
            while (offset <= png.Length - 12)
            {
                uint len = Big(png, offset);
                if (len > (uint)(png.Length - offset - 12)) throw Invalid("Truncated PNG chunk");
                int end = checked(offset + 12 + (int)len);
                string type = Encoding.ASCII.GetString(png, offset + 4, 4);
                if (Crc(png, offset + 4, end - 4) != Big(png, end - 4)) throw Invalid("PNG checksum mismatch");
                if (chunks == 0)
                {
                    if (type != "IHDR" || len != 13) throw Invalid("Invalid PNG header");
                    uint width = Big(png, offset + 8), height = Big(png, offset + 12);
                    if (width == 0 || height == 0 || width > 8192 || height > 8192 || (ulong)width * height > 32000000) throw Invalid("PNG dimensions exceed limit");
                }
                else if (type == "IHDR") throw Invalid("Duplicate PNG header");
                if (type == "IDAT") imageData = true;
                if (type == "IEND") { if (len != 0 || !imageData || end != png.Length) throw Invalid("Invalid PNG ending"); ended = true; break; }
                offset = end; chunks++;
            }
            if (!ended) throw Invalid("Missing PNG ending");
        }
        public void Dispose() { lock (gate) { stream.Dispose(); } }
    }
}
