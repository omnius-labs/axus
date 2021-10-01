using System;
using System.Diagnostics.CodeAnalysis;
using Omnius.Core;
using Omnius.Core.Pipelines;
using Omnius.Core.RocketPack;
using Omnius.Core.Serialization;
using Omnius.Xeus.Intaractors.Models;
using Omnius.Xeus.Service.Models;

namespace Omnius.Xeus.Intaractors
{
    public static class XeusMessage
    {
        private static readonly string _schema = "xeus";
        private static readonly string _nodeLocationPath = "node-location";

        public static string NodeLocationToString(NodeLocation message)
        {
            return MessageToString<NodeLocation>(_nodeLocationPath, message);
        }

        public static bool TryStringToNodeLocation(string text, [NotNullWhen(true)] out NodeLocation? message)
        {
            message = null;
            return TryStringToMessage<NodeLocation>(_nodeLocationPath, text, out message);
        }

        public static string SeedToString(Seed message)
        {
            return MessageToString<Seed>(_nodeLocationPath, message);
        }

        public static bool TryStringToSeed(string text, [NotNullWhen(true)] out Seed? message)
        {
            message = null;
            return TryStringToMessage<Seed>(_nodeLocationPath, text, out message);
        }

        private static string MessageToString<T>(string path, T message)
            where T : IRocketMessage<T>
        {
            var bytesPool = BytesPool.Shared;

            using var inBytesPipe = new BytesPipe(bytesPool);
            message.Export(inBytesPipe.Writer, bytesPool);

            using var outBytesPipe = new BytesPipe(bytesPool);
            if (!OmniMessageConverter.TryWrite(1, inBytesPipe.Reader.GetSequence(), outBytesPipe.Writer)) throw new Exception();

            return AddSchemaAndPath(path, OmniBase.Encode(outBytesPipe.Reader.GetSequence(), ConvertStringType.Base58)!);
        }

        private static bool TryStringToMessage<T>(string path, string text, [NotNullWhen(true)] out T? message)
            where T : IRocketMessage<T>
        {
            if (!TryRemoveSchemaAndPath(text, _nodeLocationPath, out var value))
            {
                message = default!;
                return false;
            }

            var bytesPool = BytesPool.Shared;

            using var inBytesPipe = new BytesPipe(bytesPool);
            if (!OmniBase.TryDecode(value, inBytesPipe.Writer))
            {
                message = default!;
                return false;
            }

            using var outBytesPipe = new BytesPipe(bytesPool);
            if (!OmniMessageConverter.TryRead(inBytesPipe.Reader.GetSequence(), out var version, outBytesPipe.Writer))
            {
                message = default!;
                return false;
            }

            message = IRocketMessage<T>.Import(outBytesPipe.Reader.GetSequence(), bytesPool);
            return true;
        }

        private static string AddSchemaAndPath(string path, string value)
        {
            return $"{_schema}:{path}/{value}";
        }

        private static bool TryRemoveSchemaAndPath(string text, string path, [NotNullWhen(true)] out string? value)
        {
            var targetPrefix = $"{_schema}:{path}/";

            value = null;
            if (!text.StartsWith(targetPrefix)) return false;

            value = text[targetPrefix.Length..];
            return true;
        }
    }
}
