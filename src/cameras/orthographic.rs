// std
use std;
use std::sync::Arc;
// pbrt
use core::camera::{Camera, CameraSample};
use core::film::Film;
use core::geometry::{Bounds2f, Point2f, Point3f, Ray, RayDifferential, Vector3f};
use core::interaction::InteractionCommon;
use core::light::VisibilityTester;
use core::medium::Medium;
use core::paramset::ParamSet;
use core::pbrt::lerp;
use core::pbrt::{Float, Spectrum};
use core::sampling::concentric_sample_disk;
use core::transform::{AnimatedTransform, Transform};

// see orthographic.h

pub struct OrthographicCamera {
    // inherited from Camera (see camera.h)
    pub camera_to_world: AnimatedTransform,
    pub shutter_open: Float,
    pub shutter_close: Float,
    pub film: Arc<Film>,
    pub medium: Option<Arc<Medium + Send + Sync>>,
    // inherited from ProjectiveCamera (see camera.h)
    pub camera_to_screen: Transform,
    pub raster_to_camera: Transform,
    pub screen_to_raster: Transform,
    pub raster_to_screen: Transform,
    pub lens_radius: Float,
    pub focal_distance: Float,
    // private data (see orthographic.h)
    pub dx_camera: Vector3f,
    pub dy_camera: Vector3f,
}

impl OrthographicCamera {
    pub fn new(
        camera_to_world: AnimatedTransform,
        screen_window: Bounds2f,
        shutter_open: Float,
        shutter_close: Float,
        lens_radius: Float,
        focal_distance: Float,
        film: Arc<Film>,
        medium: Option<Arc<Medium + Send + Sync>>,
    ) -> Self {
        // see orthographic.cpp
        let camera_to_screen: Transform = Transform::orthographic(0.0 as Float, 1.0 as Float);
        // see camera.h
        // compute projective camera screen transformations
        let scale1 = Transform::scale(
            film.full_resolution.x as Float,
            film.full_resolution.y as Float,
            1.0,
        );
        let scale2 = Transform::scale(
            1.0 / (screen_window.p_max.x - screen_window.p_min.x),
            1.0 / (screen_window.p_min.y - screen_window.p_max.y),
            1.0,
        );
        let translate = Transform::translate(&Vector3f {
            x: -screen_window.p_min.x,
            y: -screen_window.p_max.y,
            z: 0.0,
        });
        let screen_to_raster = scale1 * scale2 * translate;
        let raster_to_screen = Transform::inverse(&screen_to_raster);
        let raster_to_camera = Transform::inverse(&camera_to_screen) * raster_to_screen;
        // see orthographic.cpp
        // compute differential changes in origin for orthographic camera rays
        let dx_camera: Vector3f = raster_to_camera.transform_vector(&Vector3f {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        });
        let dy_camera: Vector3f = raster_to_camera.transform_vector(&Vector3f {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        });
        OrthographicCamera {
            camera_to_world: camera_to_world,
            shutter_open: shutter_open,
            shutter_close: shutter_close,
            film: film,
            medium: medium,
            camera_to_screen: camera_to_screen,
            raster_to_camera: raster_to_camera,
            screen_to_raster: screen_to_raster,
            raster_to_screen: raster_to_screen,
            lens_radius: lens_radius,
            focal_distance: focal_distance,
            dx_camera: dx_camera,
            dy_camera: dy_camera,
        }
    }
    pub fn create(
        params: &ParamSet,
        cam2world: AnimatedTransform,
        film: Arc<Film>,
        medium: Option<Arc<Medium + Send + Sync>>,
    ) -> Arc<Camera + Send + Sync> {
        let shutteropen: Float = params.find_one_float("shutteropen", 0.0);
        let shutterclose: Float = params.find_one_float("shutterclose", 1.0);
        // TODO: std::swap(shutterclose, shutteropen);
        assert!(shutterclose >= shutteropen);
        let lensradius: Float = params.find_one_float("lensradius", 0.0);
        let focaldistance: Float = params.find_one_float("focaldistance", 1e6);
        let frame: Float = params.find_one_float(
            "frameaspectratio",
            (film.full_resolution.x as Float) / (film.full_resolution.y as Float),
        );
        let mut screen: Bounds2f = Bounds2f::default();
        if frame > 1.0 {
            screen.p_min.x = -frame;
            screen.p_max.x = frame;
            screen.p_min.y = -1.0;
            screen.p_max.y = 1.0;
        } else {
            screen.p_min.x = -1.0;
            screen.p_max.x = 1.0;
            screen.p_min.y = -1.0 / frame;
            screen.p_max.y = 1.0 / frame;
        }
        let sw: Vec<Float> = params.find_float("screenwindow");
        if sw.len() > 0_usize {
            if sw.len() == 4 {
                screen.p_min.x = sw[0];
                screen.p_max.x = sw[1];
                screen.p_min.y = sw[2];
                screen.p_max.y = sw[3];
            } else {
                panic!("\"screenwindow\" should have four values");
            }
        }
        let camera = Arc::new(OrthographicCamera::new(
            cam2world,
            screen,
            shutteropen,
            shutterclose,
            lensradius,
            focaldistance,
            film,
            medium,
        ));
        camera
    }
}

impl Camera for OrthographicCamera {
    fn generate_ray_differential(&self, sample: &CameraSample, ray: &mut Ray) -> Float {
        // TODO: ProfilePhase prof(Prof::GenerateCameraRay);
        // compute raster and camera sample positions
        let p_film: Point3f = Point3f {
            x: sample.p_film.x,
            y: sample.p_film.y,
            z: 0.0,
        };
        let p_camera: Point3f = self.raster_to_camera.transform_point(&p_film);
        let mut diff: RayDifferential = RayDifferential {
            rx_origin: ray.o + self.dx_camera,
            ry_origin: ray.o + self.dy_camera,
            rx_direction: ray.d,
            ry_direction: ray.d,
        };
        // *ray = RayDifferential(pCamera, Vector3f(0, 0, 1));
        let mut in_ray: Ray = Ray {
            o: p_camera,
            d: Vector3f {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            t_max: std::f32::INFINITY,
            time: lerp(sample.time, self.shutter_open, self.shutter_close),
            medium: None,
            differential: Some(diff),
        };
        // modify ray for depth of field
        if self.lens_radius > 0.0 as Float {
            // sample point on lens
            let p_lens: Point2f = concentric_sample_disk(&sample.p_lens) * self.lens_radius;
            // compute point on plane of focus
            let ft: Float = self.focal_distance / in_ray.d.z;
            let p_focus: Point3f = in_ray.position(ft);
            // update ray for effect of lens
            in_ray.o = Point3f {
                x: p_lens.x,
                y: p_lens.y,
                z: 0.0 as Float,
            };
            in_ray.d = (p_focus - in_ray.o).normalize();
        }
        // compute offset rays for _OrthographicCamera_ ray differentials
        if self.lens_radius > 0.0 as Float {
            // compute _OrthographicCamera_ ray differentials accounting for lens

            // sample point on lens
            let p_lens: Point2f = concentric_sample_disk(&sample.p_lens) * self.lens_radius;
            let ft: Float = self.focal_distance / ray.d.z;
            let p_focus: Point3f = p_camera
                + self.dx_camera
                + (Vector3f {
                    x: 0.0 as Float,
                    y: 0.0 as Float,
                    z: 1.0 as Float,
                } * ft);
            diff.rx_origin = Point3f {
                x: p_lens.x,
                y: p_lens.y,
                z: 0.0 as Float,
            };
            diff.rx_direction = (p_focus - diff.rx_origin).normalize();
            let p_focus: Point3f = p_camera
                + self.dy_camera
                + (Vector3f {
                    x: 0.0 as Float,
                    y: 0.0 as Float,
                    z: 1.0 as Float,
                } * ft);
            diff.ry_origin = Point3f {
                x: p_lens.x,
                y: p_lens.y,
                z: 0.0 as Float,
            };
            diff.ry_direction = (p_focus - diff.ry_origin).normalize();
            // replace differential
            in_ray.differential = Some(diff);
        }
        // ray->medium = medium;
        if let Some(ref medium_arc) = self.medium {
            in_ray.medium = Some(medium_arc.clone());
        } else {
            in_ray.medium = None;
        }
        *ray = self.camera_to_world.transform_ray(&in_ray);
        1.0
    }
    fn we(&self, _ray: &Ray, _p_raster2: Option<&mut Point2f>) -> Spectrum {
        panic!("camera::we() is not implemented!");
        // Spectrum::default()
    }
    fn pdf_we(&self, _ray: &Ray) -> (Float, Float) {
        // let mut pdf_pos: Float = 0.0;
        // let mut pdf_dir: Float = 0.0;
        panic!("camera::pdf_we() is not implemented!");
        // (pdf_pos, pdf_dir)
    }
    fn sample_wi(
        &self,
        _iref: &InteractionCommon,
        _u: &Point2f,
        _wi: &mut Vector3f,
        _pdf: &mut Float,
        _p_raster: &mut Point2f,
        _vis: &mut VisibilityTester,
    ) -> Spectrum {
        panic!("camera::sample_wi() is not implemented!");
        // Spectrum::default()
    }
    fn get_shutter_open(&self) -> Float {
        self.shutter_open
    }
    fn get_shutter_close(&self) -> Float {
        self.shutter_close
    }
    fn get_film(&self) -> Arc<Film> {
        self.film.clone()
    }
}
