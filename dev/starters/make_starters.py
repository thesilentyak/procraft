"""Makes the starter workflows for ProCraft Diffusion: each in ComfyUI's regular (UI) format, to
open in ComfyUI so its tab sends it to the app, and in the API format, as the app reads it (a test
fixture). One description per workflow; both files come from it."""

import json
import sys

# A node: id, class, widget values (in the node's widget order), inputs fed by links
# ({input name: (from node, from slot)}), outputs (name, type), and which inputs are widgets
# given a link (their widget names, so the regular format marks them).
MODEL = ("z_image_turbo_bf16.safetensors", "qwen_3_4b.safetensors", "ae.safetensors")


def starter(paint_over: bool):
    unet, te, vae = MODEL
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(0, 0))
    node(2, "ModelSamplingAuraFlow", [3], {"model": (1, 0)}, [("MODEL", "MODEL")],
         api_widgets={"shift": 3}, pos=(340, 0))
    node(3, "CLIPLoader", [te, "lumina2", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "lumina2", "device": "default"}, pos=(0, 140))
    node(4, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(0, 300))
    node(5, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 420))
    node(6, "CLIPTextEncode", [""], {"clip": (3, 0), "text": (5, 0)}, [("CONDITIONING", "CONDITIONING")],
         widget_links=["text"], pos=(340, 140))
    node(7, "ConditioningZeroOut", [], {"conditioning": (6, 0)}, [("CONDITIONING", "CONDITIONING")], pos=(700, 140))
    node(8, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    node(9, "ImageScaleToTotalPixels", ["lanczos", 1.0, 16], {"image": (8, 0)}, [("IMAGE", "IMAGE")],
         api_widgets={"upscale_method": "lanczos", "megapixels": 1.0, "resolution_steps": 16}, pos=(340, 600))
    node(10, "VAEEncode", [], {"pixels": (9, 0), "vae": (4, 0)}, [("LATENT", "LATENT")], pos=(700, 600))
    latent = (10, 0)
    denoise = 1.0
    if paint_over:
        node(11, "ETN_KritaSelection", ["automatic", 32], {}, [("mask", "MASK"), ("active", "BOOLEAN"), ("offset x", "INT"), ("offset y", "INT")],
             api_widgets={"context": "automatic", "padding": 32}, title="Selection (from the app)", pos=(340, 760))
        node(12, "SetLatentNoiseMask", [], {"samples": (10, 0), "mask": (11, 0)}, [("LATENT", "LATENT")], pos=(1000, 600))
        latent = (12, 0)
        node(13, "ETN_Parameter", ["Strength", "number", "0.6", 0.05, 1.0], {}, [("value", "*")],
             api_widgets={"name": "Strength", "type": "number", "default": "0.6", "min": 0.05, "max": 1.0},
             title="Strength (from the app)", pos=(700, 400))
    sampler_links = {"model": (2, 0), "positive": (6, 0), "negative": (7, 0), "latent_image": latent, "seed": (8, 3)}
    sampler_widget_links = ["seed"]
    api = {"steps": 8, "cfg": 1, "sampler_name": "res_multistep", "scheduler": "simple"}
    if paint_over:
        sampler_links["denoise"] = (13, 0)
        sampler_widget_links.append("denoise")
    else:
        api["denoise"] = denoise
    node(14, "KSampler", [0, "fixed", 8, 1, "res_multistep", "simple", denoise], sampler_links, [("LATENT", "LATENT")],
         widget_links=sampler_widget_links, api_widgets=api, pos=(1300, 140))
    node(15, "VAEDecode", [], {"samples": (14, 0), "vae": (4, 0)}, [("IMAGE", "IMAGE")], pos=(1650, 140))
    node(16, "ETN_KritaOutput", [0, 0, "", "default", False], {"images": (15, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(1900, 140))
    return nodes


def minimax(edit: bool):
    """MiniMax H3 Image: a new picture at about 4 MP in the canvas's shape (the model is tuned for
    2048 x 2048 of area), or an edit of the canvas (Picture 1, its selection's part when there is
    one), the prompt an instruction. As ComfyUI's own templates, without their prompt upsampler."""
    unet = "minimax_h3_image_pruned_int8_convrot.safetensors"
    te = "qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors"
    vae = "minimax_h3_image_vae_fp16.safetensors"
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(0, 0))
    node(2, "CLIPLoader", [te, "minimax", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "minimax", "device": "default"}, pos=(0, 140))
    node(3, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(0, 300))
    node(4, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 420))
    node(5, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    links = {"clip": (2, 0), "vae": (3, 0)}
    widget_links = ["prompt"]
    if edit:
        node(6, "ETN_KritaSelection", ["automatic", 64], {}, [("mask", "MASK"), ("active", "BOOLEAN"), ("offset x", "INT"), ("offset y", "INT")],
             api_widgets={"context": "automatic", "padding": 64}, title="Selection (from the app)", pos=(0, 780))
        node(7, "StringConcatenate", ["Edit <Picture 1>: ", "", ""], {"string_b": (4, 0)}, [("STRING", "STRING")],
             widget_links=["string_b"], api_widgets={"string_a": "Edit <Picture 1>: ", "delimiter": ""}, pos=(340, 420))
        links.update({"prompt": (7, 0), "images.image_1": (5, 0)})
        api = {"width": 0, "height": 0}
    else:
        node(7, "ImageScaleToTotalPixels", ["lanczos", 4.0, 32], {"image": (5, 0)}, [("IMAGE", "IMAGE")],
             api_widgets={"upscale_method": "lanczos", "megapixels": 4.0, "resolution_steps": 32}, pos=(340, 600))
        node(8, "GetImageSize", [], {"image": (7, 0)}, [("width", "INT"), ("height", "INT"), ("batch_size", "INT")], pos=(700, 600))
        links.update({"prompt": (4, 0), "width": (8, 0), "height": (8, 1)})
        widget_links += ["width", "height"]
        api = {}
    node(9, "MiniMaxH3ImageConditioning", ["", 0, 0], links, [("positive", "CONDITIONING"), ("LATENT", "LATENT")],
         widget_links=widget_links, api_widgets=api, pos=(700, 140))
    node(10, "ConditioningZeroOut", [], {"conditioning": (9, 0)}, [("CONDITIONING", "CONDITIONING")], pos=(1050, 300))
    node(11, "KSampler", [0, "fixed", 6, 1, "euler", "simple", 1],
         {"model": (1, 0), "positive": (9, 0), "negative": (10, 0), "latent_image": (9, 1), "seed": (5, 3)},
         [("LATENT", "LATENT")], widget_links=["seed"],
         api_widgets={"steps": 6, "cfg": 1, "sampler_name": "euler", "scheduler": "simple", "denoise": 1}, pos=(1300, 140))
    node(12, "VAEDecode", [], {"samples": (11, 0), "vae": (3, 0)}, [("IMAGE", "IMAGE")], pos=(1650, 140))
    node(13, "ETN_KritaOutput", [0, 0, "", "default", False], {"images": (12, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(1900, 140))
    return nodes


def rd_flow():
    """RD Flow 64: 64 x 64 pixel art (Retro Diffusion's node pack, rd-flow-64 in custom_nodes; its
    weights download on the first run). The model wants a long description, so Qwen3-VL 4B writes
    one from the prompt with the pack's own instructions. The sprite is scaled up with hard edges and
    padded to the canvas, transparent round it."""
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    node(2, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 0))
    node(3, "ETN_Parameter", ["Background", "choice", "transparent", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Background", "type": "choice", "default": "transparent", "min": -1e10, "max": 1e10},
         title="Background (from the app)", pos=(0, 200))
    node(4, "ETN_Parameter", ["Palette", "text", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Palette", "type": "text", "default": "", "min": -1e10, "max": 1e10},
         title="Palette: hex colours, or empty (from the app)", pos=(0, 400))
    node(5, "RDFlow64SystemPrompt", ["", "transparent", 0, ""],
         {"request": (2, 0), "background": (3, 0), "seed": (1, 3), "palette": (4, 0)},
         [("system_prompt", "STRING"), ("user_message", "STRING")],
         widget_links=["request", "background", "seed", "palette"], pos=(380, 0))
    node(6, "CLIPLoader", ["qwen3vl_4b_fp8_scaled.safetensors", "krea2", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": "qwen3vl_4b_fp8_scaled.safetensors", "type": "krea2", "device": "default"},
         pos=(380, 260))
    node(7, "TextGenerateV2", ["", 700, "on", 0.6, 20, 0.95, 0, 1.05, 0, 0, False, True, "auto"],
         {"clip": (6, 0), "system_prompt": (5, 0), "prompt": (5, 1)},
         [("generated_text", "STRING"), ("thinking", "STRING")], widget_links=["prompt"],
         api_widgets={"max_length": 700, "sampling_mode": "on", "sampling_mode.temperature": 0.6,
                      "sampling_mode.top_k": 20, "sampling_mode.top_p": 0.95, "sampling_mode.min_p": 0,
                      "sampling_mode.repetition_penalty": 1.05, "sampling_mode.presence_penalty": 0,
                      "sampling_mode.seed": 0, "thinking": False, "use_default_template": True, "mtp": "auto"},
         title="Writes the long description", pos=(760, 0))
    node(8, "RDFlow64CleanCaption", ["transparent"], {"llm_output": (7, 0), "background": (3, 0)},
         [("caption", "STRING")], widget_links=["background"], pos=(1140, 0))
    node(9, "RDFlow64Loader", ["cpu", "auto", "auto", ""], {}, [("rd_flow_64", "RDFLOW64")],
         api_widgets={"text_device": "cpu", "model_device": "auto", "precision": "auto", "weights_path": ""},
         title="RD Flow 64 (its prompt reader on the CPU, leaving the GPU to ComfyUI)", pos=(1140, 260))
    node(10, "RDFlow64Generate", ["", 0, "fixed", 1, "transparent", 16, 4.0, 1, ""],
         {"rd_flow_64": (9, 0), "caption": (8, 0), "seed": (1, 3), "background": (3, 0), "palette": (4, 0)},
         [("image", "IMAGE"), ("mask", "MASK"), ("palette", "STRING"), ("rgba", "IMAGE")],
         widget_links=["caption", "seed", "background", "palette"],
         api_widgets={"batch_size": 1, "steps": 16, "guidance": 4.0, "scale": 1}, pos=(1500, 0))
    pad = {"padding_color": "black", "interpolation": "nearest-exact"}
    node(11, "ResizeAndPadImage", [512, 512, "black", "nearest-exact"],
         {"image": (10, 0), "target_width": (1, 1), "target_height": (1, 2)}, [("IMAGE", "IMAGE")],
         widget_links=["target_width", "target_height"], api_widgets=pad, title="Sharp pixels, fitted to the canvas",
         pos=(1880, 0))
    node(12, "MaskToImage", [], {"mask": (10, 1)}, [("IMAGE", "IMAGE")], pos=(1880, 260))
    node(13, "ResizeAndPadImage", [512, 512, "white", "nearest-exact"],
         {"image": (12, 0), "target_width": (1, 1), "target_height": (1, 2)}, [("IMAGE", "IMAGE")],
         widget_links=["target_width", "target_height"], api_widgets=dict(pad, padding_color="white"),
         title="Transparent round it", pos=(2200, 260))
    node(14, "ImageToMask", ["red"], {"image": (13, 0)}, [("MASK", "MASK")], api_widgets={"channel": "red"},
         pos=(2520, 260))
    node(15, "JoinImageWithAlpha", [], {"image": (11, 0), "alpha": (14, 0)}, [("IMAGE", "IMAGE")], pos=(2520, 0))
    node(16, "ETN_KritaOutput", [0, 0, "Pixel art", "default", False], {"images": (15, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "Pixel art", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(2840, 0))
    return nodes


def pixel_edit():
    """Pixel art edit: MiniMax H3 edits the canvas (or its selection's part), blown up with hard
    edges to about 2048 pixels across first, and SpriteFusion's Pixel Snapper (its node pack in
    custom_nodes) puts the result back on the canvas's own pixel grid with a limited palette, the
    plain background round the sprite taken out."""
    unet = "minimax_h3_image_pruned_int8_convrot.safetensors"
    te = "qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors"
    vae = "minimax_h3_image_vae_fp16.safetensors"
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    node(1, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 600))
    node(2, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt (from the app)", pos=(0, 420))
    node(3, "ETN_Parameter", ["Palette", "text", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Palette", "type": "text", "default": "", "min": -1e10, "max": 1e10},
         title="Palette: hex colours, or empty (from the app)", pos=(1900, 700))
    node(4, "ETN_Parameter", ["Transparent background", "toggle", "true", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Transparent background", "type": "toggle", "default": "true", "min": -1e10, "max": 1e10},
         title="Transparent background (from the app)", pos=(1900, 900))
    node(5, "ETN_KritaSelection", ["automatic", 16], {}, [("mask", "MASK"), ("active", "BOOLEAN"), ("offset x", "INT"), ("offset y", "INT")],
         api_widgets={"context": "automatic", "padding": 16}, title="Selection (from the app)", pos=(0, 800))
    node(6, "ComfyMathExpression", ["max(1, floor(2048 / max(a, b)))"], {"values.a": (1, 1), "values.b": (1, 2)},
         [("FLOAT", "FLOAT"), ("INT", "INT"), ("BOOL", "BOOLEAN")],
         api_widgets={"expression": "max(1, floor(2048 / max(a, b)))"}, title="How much to blow it up", pos=(340, 600))
    node(7, "ImageScaleBy", ["nearest-exact", 1.0], {"image": (1, 0), "scale_by": (6, 0)}, [("IMAGE", "IMAGE")],
         widget_links=["scale_by"], api_widgets={"upscale_method": "nearest-exact"}, title="Hard-edged, big enough for MiniMax",
         pos=(700, 600))
    node(8, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(0, 0))
    node(9, "CLIPLoader", [te, "minimax", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "minimax", "device": "default"}, pos=(0, 140))
    node(10, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(0, 300))
    prefix = "Edit <Picture 1>, keeping it pixel art on a plain background: "
    node(11, "StringConcatenate", [prefix, "", ""], {"string_b": (2, 0)}, [("STRING", "STRING")],
         widget_links=["string_b"], api_widgets={"string_a": prefix, "delimiter": ""}, pos=(340, 420))
    node(12, "MiniMaxH3ImageConditioning", ["", 0, 0], {"clip": (9, 0), "vae": (10, 0), "prompt": (11, 0), "images.image_1": (7, 0)},
         [("positive", "CONDITIONING"), ("LATENT", "LATENT")], widget_links=["prompt"],
         api_widgets={"width": 0, "height": 0}, pos=(1050, 140))
    node(13, "ConditioningZeroOut", [], {"conditioning": (12, 0)}, [("CONDITIONING", "CONDITIONING")], pos=(1400, 300))
    node(14, "KSampler", [0, "fixed", 6, 1, "euler", "simple", 1],
         {"model": (8, 0), "positive": (12, 0), "negative": (13, 0), "latent_image": (12, 1), "seed": (1, 3)},
         [("LATENT", "LATENT")], widget_links=["seed"],
         api_widgets={"steps": 6, "cfg": 1, "sampler_name": "euler", "scheduler": "simple", "denoise": 1}, pos=(1650, 140))
    node(15, "VAEDecode", [], {"samples": (14, 0), "vae": (10, 0)}, [("IMAGE", "IMAGE")], pos=(2000, 140))
    node(16, "GetImageSize", [], {"image": (15, 0)}, [("width", "INT"), ("height", "INT"), ("batch_size", "INT")],
         pos=(2000, 300))
    node(17, "ComfyMathExpression", ["a / b"], {"values.a": (16, 0), "values.b": (1, 1)},
         [("FLOAT", "FLOAT"), ("INT", "INT"), ("BOOL", "BOOLEAN")], api_widgets={"expression": "a / b"},
         title="The size of one canvas pixel in it", pos=(2300, 300))
    node(18, "SpriteFusionPixelSnapper",
         [24, 0.0, "exact_size", 1, 64, 64, "connected_chroma_key", "auto", 32, "majority", "", "uniform"],
         {"image": (15, 0), "pixel_size": (17, 0), "exact_width": (1, 1), "exact_height": (1, 2), "custom_palette": (3, 0)},
         [("image", "IMAGE"), ("grid_width", "INT"), ("grid_height", "INT"), ("mask", "MASK"), ("key_color_used", "STRING")],
         widget_links=["pixel_size", "exact_width", "exact_height", "custom_palette"],
         api_widgets={"colors": 24, "output_mode": "exact_size", "output_scale": 1, "transparency": "connected_chroma_key",
                      "key_color": "auto", "key_tolerance": 32, "cell_method": "majority", "grid_mode": "uniform"},
         title="Back onto the canvas's pixels", pos=(2600, 140))
    node(19, "SolidMask", [0.0, 512, 512], {"width": (1, 1), "height": (1, 2)}, [("MASK", "MASK")],
         widget_links=["width", "height"], api_widgets={"value": 0.0}, title="Nothing transparent", pos=(2600, 600))
    node(20, "ComfySwitchNode", [True], {"switch": (4, 0), "on_true": (18, 3), "on_false": (19, 0)}, [("output", "MASK")],
         widget_links=["switch"], pos=(2950, 400))
    node(21, "JoinImageWithAlpha", [], {"image": (18, 0), "alpha": (20, 0)}, [("IMAGE", "IMAGE")], pos=(3250, 140))
    node(22, "ETN_KritaOutput", [0, 0, "Pixel edit", "default", False], {"images": (21, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "Pixel edit", "batch_mode": "default", "resize_canvas": False},
         title="Back to the app", pos=(3550, 140))
    return nodes


def pixel_animation():
    """Pixel animation: MiniMax H3 animates the canvas's sprite. The sprite is blown up with hard
    edges by a whole number, so its longest side is 8 times the canvas's, between 768 and 1280
    pixels, and centred on white, the sides a multiple of 32; it's the video's first and last
    frame, so the move loops. The sprite's part of every few video frames is put back on the
    canvas's pixels by SpriteFusion's Pixel Snapper, white keyed out, and the frames go back as an
    animation (batch mode "animation"): the app puts them on the sprite's own colours, keeps its
    pixels where they barely moved, and makes them one result whose Apply adds them as frames,
    named after the move."""
    unet = "minimax_h3_fl2va_pruned_int8_convrot.safetensors"
    lora = "minimax_h3_fl2v_turbo_8step_v1.0_comfyui_bf16.safetensors"
    te = "qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors"
    vae = "minimax_h3_video_vae_fp16.safetensors"
    nodes = []

    def node(nid, cls, widgets, links, outputs, widget_links=(), api_widgets=None, title=None, pos=(0, 0)):
        nodes.append(dict(id=nid, cls=cls, widgets=widgets, links=links, outputs=outputs,
                          widget_links=set(widget_links), api=api_widgets or {}, title=title, pos=pos))

    maths = [("FLOAT", "FLOAT"), ("INT", "INT"), ("BOOL", "BOOLEAN")]

    def math(nid, expression, values, title, pos):
        node(nid, "ComfyMathExpression", [expression], {f"values.{k}": v for k, v in values.items()}, maths,
             api_widgets={"expression": expression}, title=title, pos=pos)

    node(1, "ETN_KritaCanvas", [], {}, [("image", "IMAGE"), ("width", "INT"), ("height", "INT"), ("seed", "INT"), ("mask", "MASK")],
         title="Canvas (from the app)", pos=(0, 700))
    node(2, "ETN_Parameter", ["Prompt", "prompt (positive)", "", -1e10, 1e10], {}, [("value", "*")],
         api_widgets={"name": "Prompt", "type": "prompt (positive)", "default": "", "min": -1e10, "max": 1e10},
         title="Prompt: the move (from the app)", pos=(0, 0))
    node(3, "ETN_Parameter", ["Frames", "number (integer)", "8", 2, 24], {}, [("value", "*")],
         api_widgets={"name": "Frames", "type": "number (integer)", "default": "8", "min": 2, "max": 24},
         title="Frames (from the app)", pos=(0, 220))
    node(4, "ETN_Parameter", ["Seconds", "number", "1.6", 1, 5], {}, [("value", "*")],
         api_widgets={"name": "Seconds", "type": "number", "default": "1.6", "min": 1, "max": 5},
         title="Seconds (from the app)", pos=(0, 440))
    canvas_w, canvas_h = (1, 1), (1, 2)
    math(11, "max(1, floor(min(1280, max(768, 8 * max(a, b))) / max(a, b)))", {"a": canvas_w, "b": canvas_h},
         "How much to blow it up", (400, 700))
    k_int, k_float = (11, 1), (11, 0)
    math(12, "ceil(a * b / 32) * 32", {"a": canvas_w, "b": k_int}, "Video width", (400, 900))
    math(13, "ceil(a * b / 32) * 32", {"a": canvas_h, "b": k_int}, "Video height", (400, 1100))
    math(14, "floor((c - a * b) / 2)", {"a": canvas_w, "b": k_int, "c": (12, 1)}, "Sprite x in the video", (400, 1300))
    math(15, "floor((c - a * b) / 2)", {"a": canvas_h, "b": k_int, "c": (13, 1)}, "Sprite y in the video", (400, 1500))
    math(16, "a * b", {"a": canvas_w, "b": k_int}, "Sprite width, blown up", (400, 1700))
    math(17, "a * b", {"a": canvas_h, "b": k_int}, "Sprite height, blown up", (400, 1900))
    node(18, "ImageScaleBy", ["nearest-exact", 1.0], {"image": (1, 0), "scale_by": k_float}, [("IMAGE", "IMAGE")],
         widget_links=["scale_by"], api_widgets={"upscale_method": "nearest-exact"}, title="Blown up with hard edges",
         pos=(800, 700))
    node(19, "EmptyImage", [512, 512, 1, 16777215], {"width": (12, 1), "height": (13, 1)}, [("IMAGE", "IMAGE")],
         widget_links=["width", "height"], api_widgets={"batch_size": 1, "color": 16777215}, title="White video frame",
         pos=(800, 900))
    node(20, "ImageCompositeMasked", [0, 0, False], {"destination": (19, 0), "source": (18, 0), "x": (14, 1), "y": (15, 1)},
         [("IMAGE", "IMAGE")], widget_links=["x", "y"], api_widgets={"resize_source": False},
         title="First (and last) frame", pos=(800, 1100))
    before = "A 2D pixel art game sprite animation:"
    after = ("The camera is completely still, the character stays in the centre of the frame at the same size, and "
             "the plain background never changes. Crisp blocky pixel art with dark outlines, flat colours, "
             "no motion blur.")
    node(21, "StringConcatenate", [before, "", " "], {"string_b": (2, 0)}, [("STRING", "STRING")],
         widget_links=["string_b"], api_widgets={"string_a": before, "delimiter": " "}, pos=(400, 0))
    node(22, "StringConcatenate", ["", after, " "], {"string_a": (21, 0)}, [("STRING", "STRING")],
         widget_links=["string_a"], api_widgets={"string_b": after, "delimiter": " "}, title="The whole prompt",
         pos=(800, 0))
    length = "max(5, round(a * 24)) + (5 - (max(5, round(a * 24)) % 17)) % 17"
    math(23, length, {"a": (4, 0)}, "Video length in frames (17k + 5)", (400, 440))
    node(30, "UNETLoader", [unet, "default"], {}, [("MODEL", "MODEL")],
         api_widgets={"unet_name": unet, "weight_dtype": "default"}, pos=(1200, 0))
    node(31, "LoraLoaderModelOnly", [lora, 1.0], {"model": (30, 0)}, [("MODEL", "MODEL")],
         api_widgets={"lora_name": lora, "strength_model": 1.0}, pos=(1200, 160))
    node(32, "CLIPLoader", [te, "minimax", "default"], {}, [("CLIP", "CLIP")],
         api_widgets={"clip_name": te, "type": "minimax", "device": "default"}, pos=(1200, 320))
    node(33, "VAELoader", [vae], {}, [("VAE", "VAE")], api_widgets={"vae_name": vae}, pos=(1200, 480))
    node(34, "MiniMaxH3ImageToVideo", ["", 768, 768, 39],
         {"clip": (32, 0), "vae": (33, 0), "prompt": (22, 0), "width": (12, 1), "height": (13, 1), "length": (23, 1),
          "first_frame": (20, 0), "last_frame": (20, 0)},
         [("positive", "CONDITIONING"), ("latent", "LATENT")], widget_links=["prompt", "width", "height", "length"],
         title="The same first and last frame: it loops", pos=(1600, 0))
    node(35, "RandomNoise", [0, "fixed"], {"noise_seed": (1, 3)}, [("NOISE", "NOISE")], widget_links=["noise_seed"],
         pos=(1600, 300))
    node(36, "BasicGuider", [], {"model": (31, 0), "conditioning": (34, 0)}, [("GUIDER", "GUIDER")], pos=(1600, 440))
    node(37, "KSamplerSelect", ["res_multistep"], {}, [("SAMPLER", "SAMPLER")],
         api_widgets={"sampler_name": "res_multistep"}, pos=(1600, 560))
    node(38, "BasicScheduler", ["simple", 8, 1.0], {"model": (31, 0)}, [("SIGMAS", "SIGMAS")],
         api_widgets={"scheduler": "simple", "steps": 8, "denoise": 1.0}, pos=(1600, 680))
    node(39, "SamplerCustomAdvanced", [], {"noise": (35, 0), "guider": (36, 0), "sampler": (37, 0), "sigmas": (38, 0),
                                           "latent_image": (34, 1)},
         [("output", "LATENT"), ("denoised_output", "LATENT")], pos=(2000, 0))
    node(40, "VAEDecode", [], {"samples": (39, 0), "vae": (33, 0)}, [("IMAGE", "IMAGE")], pos=(2000, 300))
    node(41, "ImageCrop", [512, 512, 0, 0], {"image": (40, 0), "width": (16, 1), "height": (17, 1), "x": (14, 1), "y": (15, 1)},
         [("IMAGE", "IMAGE")], widget_links=["width", "height", "x", "y"], title="Just the sprite's part", pos=(2000, 440))
    math(42, "a - 1", {"a": (23, 1)}, "Without the repeated last frame", (2000, 700))
    node(43, "GetImageRangeFromBatch", [0, 1], {"images": (41, 0), "num_frames": (42, 1)}, [("IMAGE", "IMAGE"), ("MASK", "MASK")],
         widget_links=["num_frames"], api_widgets={"start_index": 0}, pos=(2400, 0))
    math(44, "max(1, round(a / b))", {"a": (42, 1), "b": (3, 0)}, "Keep every nth frame", (2400, 200))
    node(45, "RebatchImages", [1], {"images": (43, 0), "batch_size": (44, 1)}, [("IMAGE", "IMAGE")],
         widget_links=["batch_size"], pos=(2400, 400))
    node(46, "ImageFromBatch", [0, 1], {"image": (45, 0)}, [("IMAGE", "IMAGE")],
         api_widgets={"batch_index": 0, "length": 1}, pos=(2400, 560))
    node(47, "ImageListToBatch+", [], {"image": (46, 0)}, [("IMAGE", "IMAGE")], title="The frames kept", pos=(2400, 720))
    node(52, "SpriteFusionPixelSnapper",
         [256, 1.0, "exact_size", 1, 64, 64, "connected_chroma_key", "#FFFFFF", 48, "majority", "", "uniform"],
         {"image": (47, 0), "pixel_size": k_float, "exact_width": canvas_w, "exact_height": canvas_h},
         [("image", "IMAGE"), ("grid_width", "INT"), ("grid_height", "INT"), ("mask", "MASK"), ("key_color_used", "STRING")],
         widget_links=["pixel_size", "exact_width", "exact_height"],
         api_widgets={"colors": 256, "output_mode": "exact_size", "output_scale": 1, "transparency": "connected_chroma_key",
                      "key_color": "#FFFFFF", "key_tolerance": 48, "cell_method": "majority", "custom_palette": "",
                      "grid_mode": "uniform"},
         title="Back onto the canvas's pixels (the app puts them on the sprite's colours)", pos=(2800, 0))
    node(53, "JoinImageWithAlpha", [], {"image": (52, 0), "alpha": (52, 3)}, [("IMAGE", "IMAGE")],
         title="White taken out", pos=(3200, 0))
    node(55, "ETN_KritaOutput", [0, 0, "", "animation", False], {"images": (53, 0)}, [],
         api_widgets={"x": 0, "y": 0, "name": "", "batch_mode": "animation", "resize_canvas": False},
         title="Back to the app, as frames", pos=(3600, 0))
    return nodes


def widget_inputs(n):
    # the inputs that are widgets given a link
    types = {"text": "STRING", "seed": "INT", "denoise": "FLOAT", "prompt": "STRING", "width": "INT",
             "height": "INT", "string_b": "STRING", "request": "STRING", "background": "COMBO",
             "palette": "STRING", "caption": "STRING", "target_width": "INT", "target_height": "INT",
             "scale_by": "FLOAT", "pixel_size": "FLOAT", "exact_width": "INT", "exact_height": "INT",
             "custom_palette": "STRING", "switch": "BOOLEAN", "x": "INT", "y": "INT", "length": "INT",
             "noise_seed": "INT", "num_frames": "INT", "batch_size": "INT", "columns": "INT", "cell_width": "INT",
             "cell_height": "INT", "cols": "INT", "string_a": "STRING"}
    return [(w, types.get(w, "*")) for w in n["widget_links"]]


def to_ui(nodes, name):
    links = []
    ui_nodes = []
    link_id = 0
    by_id = {n["id"]: n for n in nodes}
    out_links = {}
    for n in nodes:
        for inp, (src, slot) in n["links"].items():
            link_id += 1
            typ = by_id[src]["outputs"][slot][1]
            links.append([link_id, src, slot, n["id"], inp, typ])
            out_links.setdefault((src, slot), []).append(link_id)
    for order, n in enumerate(nodes):
        inputs = []
        slot = 0
        index = {}
        for inp, (src, s) in n["links"].items():
            lid = next(l[0] for l in links if l[3] == n["id"] and l[4] == inp)
            entry = {"name": inp, "type": by_id[src]["outputs"][s][1], "link": lid}
            if inp in n["widget_links"]:
                entry["widget"] = {"name": inp}
                entry["type"] = dict(widget_inputs(n)).get(inp, "*")
            inputs.append(entry)
            index[inp] = slot
            slot += 1
        outputs = [{"name": oname, "type": otype, "links": out_links.get((n["id"], i), []), "slot_index": i}
                   for i, (oname, otype) in enumerate(n["outputs"])]
        ui = {"id": n["id"], "type": n["cls"], "pos": list(n["pos"]), "size": [300, 100], "flags": {}, "order": order,
              "mode": 0, "inputs": inputs, "outputs": outputs, "properties": {"Node name for S&R": n["cls"]},
              "widgets_values": n["widgets"]}
        if n["title"]:
            ui["title"] = n["title"]
        ui_nodes.append(ui)
    # links refer to target slots by index
    for l in links:
        target = next(x for x in ui_nodes if x["id"] == l[3])
        l[4] = next(i for i, e in enumerate(target["inputs"]) if e["name"] == l[4])
    note = {"id": 99, "type": "MarkdownNote", "pos": [0, -260], "size": [620, 220], "flags": {}, "order": len(ui_nodes),
            "mode": 0, "inputs": [], "outputs": [], "properties": {},
            "widgets_values": [f"## {name}\n\nMade for ProCraft Diffusion. Keep this workflow **open in ComfyUI**: the app on your phone gets it as the **ComfyUI (live)** card. On the phone, hold that card and choose **Keep** to keep it for good.\n\nThe nodes marked *(from the app)* are filled in by the app: your canvas, your prompt, the selection. **Back to the app** sends the result to your phone. Pressing Run here does nothing useful."]}
    ui_nodes.append(note)
    return {"id": "procraft-" + name.lower().replace(" ", "-"), "revision": 0, "last_node_id": 99,
            "last_link_id": link_id, "nodes": ui_nodes, "links": links, "groups": [], "config": {},
            "extra": {"ds": {"scale": 0.7, "offset": [60, 320]}}, "version": 0.4}


def to_api(nodes):
    out = {}
    for n in nodes:
        inputs = dict(n["api"])
        if n["cls"] == "CLIPTextEncode":
            pass
        for inp, (src, slot) in n["links"].items():
            inputs[inp] = [str(src), slot]
        out[str(n["id"])] = {"class_type": n["cls"], "inputs": inputs}
    return out


if __name__ == "__main__":
    where = sys.argv[1]
    made = [
        (starter(False), "ProCraft - Z-Image new picture"),
        (starter(True), "ProCraft - Z-Image paint over"),
        (minimax(False), "ProCraft - MiniMax new picture"),
        (minimax(True), "ProCraft - MiniMax edit"),
        (rd_flow(), "ProCraft - Pixel art (RD Flow 64)"),
        (pixel_edit(), "ProCraft - Pixel art edit (MiniMax)"),
        (pixel_animation(), "ProCraft - Pixel animation (MiniMax H3)"),
    ]
    for nodes, name in made:
        json.dump(to_ui(nodes, name), open(f"{where}/{name}.json", "w"), indent=2)
        json.dump(to_api(nodes), open(f"{where}/{name} (API).json", "w"), indent=2)
        print("made", name)
