// A C surface over the vendored msdfgen core (alloy/vendor/msdfgen, compiled
// with this file by alloy/build.rs) for the glyph engine's distance-field
// cells: a shape built edge by edge, its bounds, and one MTSDF generation
// into a caller-owned float buffer. Bound from Rust in
// src/rendertree/text/glyphs/ffi.rs; the safe layer over that is msdf.rs,
// which owns every rule about outlines (closing contours, winding).
//
// Coordinates are msdfgen's: y up, the inside on the right-hand side of
// travel (outer contours clockwise, the TrueType convention). The generated
// rows come out top first: the bitmap is declared y-down and msdfgen orders
// the rows itself.
#include "msdfgen.h"

extern "C" {

struct msdf_shape {
  msdfgen::Shape shape;
};

msdf_shape *msdf_shape_new(void) {
  return new msdf_shape();
}

void msdf_shape_free(msdf_shape *shape) {
  delete shape;
}

// Begin a contour; the edges that follow belong to it.
void msdf_shape_contour(msdf_shape *shape) {
  shape->shape.addContour();
}

static msdfgen::Contour &current_contour(msdf_shape *shape) {
  if (shape->shape.contours.empty())
    shape->shape.addContour();
  return shape->shape.contours.back();
}

void msdf_shape_line(msdf_shape *shape, double x0, double y0, double x1, double y1) {
  current_contour(shape).addEdge(msdfgen::EdgeHolder(msdfgen::Point2(x0, y0), msdfgen::Point2(x1, y1)));
}

void msdf_shape_quad(msdf_shape *shape, double x0, double y0, double cx, double cy, double x1, double y1) {
  current_contour(shape).addEdge(msdfgen::EdgeHolder(msdfgen::Point2(x0, y0), msdfgen::Point2(cx, cy), msdfgen::Point2(x1, y1)));
}

void msdf_shape_cubic(msdf_shape *shape, double x0, double y0, double c0x, double c0y, double c1x, double c1y, double x1, double y1) {
  current_contour(shape).addEdge(msdfgen::EdgeHolder(msdfgen::Point2(x0, y0), msdfgen::Point2(c0x, c0y), msdfgen::Point2(c1x, c1y), msdfgen::Point2(x1, y1)));
}

// Reverse every contour: a shape wound the other way round.
void msdf_shape_reverse(msdf_shape *shape) {
  for (std::vector<msdfgen::Contour>::iterator contour = shape->shape.contours.begin(); contour != shape->shape.contours.end(); ++contour)
    contour->reverse();
}

// msdfgen's precondition for generation: convergent edges pushed apart,
// single-edge contours split.
void msdf_shape_normalize(msdf_shape *shape) {
  shape->shape.normalize();
}

// The shape's bounds, y up. 0 when the shape has no edges.
int msdf_shape_bounds(msdf_shape *shape, double *left, double *bottom, double *right, double *top) {
  if (shape->shape.edgeCount() == 0)
    return 0;
  msdfgen::Shape::Bounds bounds = shape->shape.getBounds();
  *left = bounds.l;
  *bottom = bounds.b;
  *right = bounds.r;
  *top = bounds.t;
  return 1;
}

// Generate the shape's MTSDF into `pixels` (width * height * 4 floats, rgba,
// rows top first): the shape's units map to texels by `scale` after
// `translate` (msdfgen's projection, scale * (p + translate)); `range` is
// the distance range in texels, the field 0.5 at the edge and spanning the
// range. Edges are coloured first (edgeColoringSimple at `angle` radians).
// 0 when the shape is empty or not a closed outline, or the buffer has no
// size.
int msdf_generate_mtsdf(msdf_shape *shape, double angle, float *pixels, int width, int height, double scale, double translate_x, double translate_y, double range) {
  if (!pixels || width <= 0 || height <= 0 || shape->shape.edgeCount() == 0 || !shape->shape.validate())
    return 0;
  msdfgen::edgeColoringSimple(shape->shape, angle);
  msdfgen::BitmapSection<float, 4> output(pixels, width, height, msdfgen::Y_DOWNWARD);
  msdfgen::SDFTransformation transformation(
    msdfgen::Projection(msdfgen::Vector2(scale), msdfgen::Vector2(translate_x, translate_y)),
    msdfgen::DistanceMapping(msdfgen::Range(range)));
  msdfgen::generateMTSDF(output, shape->shape, transformation);
  return 1;
}

}
