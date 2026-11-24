#include <CoreFoundation/CoreFoundation.h>
#include <CoreGraphics/CoreGraphics.h>
#include <ImageIO/ImageIO.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void fail(const char *message) {
    fprintf(stderr, "%s\n", message);
    exit(EXIT_FAILURE);
}

static CFURLRef file_url(const char *path) {
    CFURLRef url = CFURLCreateFromFileSystemRepresentation(
        NULL, (const UInt8 *)path, (CFIndex)strlen(path), false);
    if (!url) {
        fail("应用图标路径无效");
    }
    return url;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fail("用法: draw-icon-tile 原图.png 图标.png");
    }

    CFURLRef input = file_url(argv[1]);
    CGImageSourceRef source = CGImageSourceCreateWithURL(input, NULL);
    CFRelease(input);
    if (!source) {
        fail("无法读取应用图标原图");
    }
    CGImageRef artwork = CGImageSourceCreateImageAtIndex(source, 0, NULL);
    CFRelease(source);
    if (!artwork || CGImageGetWidth(artwork) != CGImageGetHeight(artwork)) {
        fail("应用图标原图需要是可读取的正方形图片");
    }

    const size_t size = 1024;
    CGColorSpaceRef color_space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef context = CGBitmapContextCreate(
        NULL, size, size, 8, 0, color_space, kCGImageAlphaPremultipliedLast);
    CGColorSpaceRelease(color_space);
    if (!context) {
        fail("无法建立应用图标画布");
    }

    // Compose the original artwork on a transparent legacy macOS icon tile.
    CGRect canvas = CGRectMake(0, 0, size, size);
    CGRect tile = CGRectInset(canvas, 92, 92);
    CGContextClearRect(context, canvas);
    CGPathRef outline = CGPathCreateWithRoundedRect(tile, 186, 186, NULL);
    CGContextAddPath(context, outline);
    CGPathRelease(outline);
    CGContextClip(context);
    CGContextSetInterpolationQuality(context, kCGInterpolationHigh);
    CGContextDrawImage(context, tile, artwork);
    CGImageRelease(artwork);

    CGImageRef tile_image = CGBitmapContextCreateImage(context);
    CGContextRelease(context);
    CFURLRef output = file_url(argv[2]);
    CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
        output, CFSTR("public.png"), 1, NULL);
    CFRelease(output);
    if (!tile_image || !destination) {
        fail("无法写入圆角应用图标");
    }
    CGImageDestinationAddImage(destination, tile_image, NULL);
    CGImageRelease(tile_image);
    if (!CGImageDestinationFinalize(destination)) {
        fail("圆角应用图标写入失败");
    }
    CFRelease(destination);
    return EXIT_SUCCESS;
}
