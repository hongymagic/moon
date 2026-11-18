import Image from "next/image";
import lunar from "../public/lunar.webp";

/** The orbit crosses in front of the Moon only on its near side. */
export default function LunarPlate() {
  return (
    <div className="lunar-plate" aria-hidden="true">
      <div className="plate-grid" />
      <div className="star-field" />
      <div className="plate-center">
        <i className="range-ring outer-ring" />
        <i className="range-ring inner-ring" />
        <span className="orbit orbit-back" />
        <div className="lunar-globe">
          <Image
            src={lunar}
            alt=""
            priority
            sizes="(max-width: 640px) 68vw, (max-width: 960px) 38vw, 490px"
          />
        </div>
        <span className="orbit orbit-front" />
        <span className="orbit-satellite" />
      </div>
      <i className="reticle reticle-one" />
      <i className="reticle reticle-two" />
      <i className="reticle reticle-three" />
      <i className="reticle reticle-four" />
    </div>
  );
}
